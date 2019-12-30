use {
    cranelift_codegen::{
        ir::{
            types::I32, AbiParam, ExternalName, Function, InstBuilder, Signature, StackSlotData,
            StackSlotKind,
        },
        isa::CallConv,
        settings::{self},
        verify_function,
    },
    cranelift_frontend::{FunctionBuilder, FunctionBuilderContext},
    cranelift_module::Linkage,
    pest::Parser,
    pnc::{codegen, Compiler, Penance, Rule},
    std::{
        fs::File,
        io::{Write, Read},
        path::{Path, PathBuf},
        process::{exit, Command},
    },
    structopt::StructOpt,
};

#[derive(Debug, StructOpt)]
#[structopt(name = "pnc", about = "A small CL compiler.")]
pub struct Opts {
    #[structopt(name = "FILE", parse(from_os_str))]
    input: PathBuf,
}

fn main() {
    let opts = Opts::from_args();

    let mut buf = String::new();
    File::open(opts.input).unwrap().read_to_string(&mut buf).unwrap();

    if let Ok(stream) = Penance::parse(Rule::file, &buf) {
        let mut compiler = Compiler::new();
        let parsed = stream.last().unwrap();

        let signature = {
            let mut signature = Signature::new(CallConv::SystemV);
            signature.returns.push(AbiParam::new(I32));
            signature
        };

        let flags = settings::Flags::new(settings::builder());
        let mut main = Function::with_name_signature(ExternalName::user(0, 0), signature.clone());

        let mut ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut main, &mut ctx);

        let exit_status_slot =
            { builder.create_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 4)) };

        let start = builder.create_ebb();
        builder.switch_to_block(start);
        let zero = builder.ins().iconst(I32, 0);
        builder.ins().stack_store(zero, exit_status_slot, 0);

        codegen(parsed, &mut builder).unwrap();

        // Finalize
        let end = builder.create_ebb();
        builder.ins().jump(end, &[]);
        builder.switch_to_block(end);

        let exit_status = builder.ins().stack_load(I32, exit_status_slot, 0);
        builder.ins().return_(&[exit_status]);

        builder.seal_all_blocks();
        builder.finalize();
        println!("{:?}", main);

        verify_function(&main, &flags).unwrap();

        compiler
            .define_function(main, "main", Linkage::Export, signature)
            .unwrap();

        let product = compiler.module.finish();

        let obj_file = "a.obj";
        let output = "a.out";

        // Assemble and produe object binaries.
        let bytes = product.emit().unwrap();
        File::create(Path::new(output))
            .unwrap()
            .write_all(&bytes)
            .unwrap();

        // Link the object file using host linker
        Command::new("cc")
            .args(&[&Path::new(obj_file), Path::new("-o"), Path::new(output)])
            .status()
            .unwrap();
    } else {
        eprintln!("Failed to compile");
        exit(1);
    }
}
