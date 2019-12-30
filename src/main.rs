#[macro_use]
extern crate pest_derive;
extern crate pest;

use {
    cranelift_codegen::{
        ir::{
            immediates::Imm64,
            types::{B8, I32, I64, I8},
            AbiParam, ExternalName, Function, InstBuilder, Signature, StackSlotData, StackSlotKind,
        },
        isa::{self, CallConv},
        settings::{self, Configurable},
        verify_function, Context,
    },
    cranelift_faerie::{FaerieBackend, FaerieBuilder, FaerieTrapCollection},
    cranelift_frontend::{FunctionBuilder, FunctionBuilderContext},
    cranelift_module::{Backend, Linkage, Module as CraneliftModule},
    pest::{iterators::Pair, Parser},
    std::{
        fs::File,
        io::Write,
        path::Path,
        process::{exit, Command},
    },
};

pub type Product = <FaerieBackend as Backend>::Product;
pub type Module = CraneliftModule<FaerieBackend>;

#[derive(Parser)]
#[grammar = "pnc.pest"]
pub struct Penance;

pub struct Compiler {
    pub module: Module,
}

impl Compiler {
    pub fn new() -> Self {
        let mut flags_builder = settings::builder();
        // allow creating shared libraries
        flags_builder
            .enable("is_pic")
            .expect("is_pic should be a valid option");
        // use debug assertions
        flags_builder
            .enable("enable_verifier")
            .expect("enable_verifier should be a valid option");
        // minimal optimizations
        flags_builder
            .set("opt_level", "speed")
            .expect("opt_level: speed should be a valid option");

        let isa = isa::lookup(target_lexicon::Triple::host())
            .unwrap()
            .finish(settings::Flags::new(flags_builder));

        let builder = FaerieBuilder::new(
            isa,
            "<empty>".to_string(),
            FaerieTrapCollection::Disabled,
            cranelift_module::default_libcall_names(),
        )
        .unwrap();

        Self {
            module: Module::new(builder),
        }
    }

    pub fn define_function(
        &mut self,
        func: Function,
        name: &str,
        linkage: Linkage,
        signature: Signature,
    ) -> Result<(), ()> {
        let fid = self
            .module
            .declare_function(name, linkage, &signature)
            .unwrap();

        let mut ctx = Context::for_function(func);
        self.module.define_function(fid, &mut ctx).unwrap();
        Ok(())
    }
}

fn codegen(pair: Pair<Rule>, mut builder: &mut FunctionBuilder) -> Result<(), ()> {
    match dbg!(pair.as_rule()) {
        Rule::file => {
            for inner in pair.into_inner() {
                codegen(inner, &mut builder)?;
            }
        }

        Rule::s_expr | Rule::atom => {
            let block = builder.create_ebb();
            builder.ins().jump(block, &[]);
            builder.switch_to_block(block);

            for inner in pair.into_inner() {
                codegen(inner, &mut builder)?;
            }
        }

        Rule::boolean => {
            let data = StackSlotData::new(StackSlotKind::ExplicitSlot, 0);
            let slot = builder.create_stack_slot(data);

            {
                let false_ = builder.ins().bconst(B8, false);
                let int_false = builder.ins().bint(I8, false_);
                builder.ins().stack_store(int_false, slot, 0);
            }
        }

        Rule::number => {
            let data = StackSlotData::new(StackSlotKind::ExplicitSlot, 0);
            let slot = builder.create_stack_slot(data);

            let raw_n: i64 = dbg!(pair.as_str().parse().unwrap());
            let encoded = builder.ins().iconst(I64, Imm64::new(raw_n));
            builder.ins().stack_store(encoded, slot, 0);
        }

        Rule::EOI => {}
        _ => unreachable!("You've gone and fucked it now have't you?"),
    }

    Ok(())
}

fn main() {
    if let Ok(stream) = Penance::parse(Rule::file, "(100)") {
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
