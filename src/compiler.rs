use {
    cranelift_codegen::{
        ir::{
            types::I32, AbiParam, ExternalName, Function, InstBuilder, Signature, StackSlotData,
            StackSlotKind,
        },
        isa::{self, CallConv},
        settings::{self, Configurable},
        verify_function, Context,
    },
    cranelift_faerie::{FaerieBackend, FaerieBuilder, FaerieTrapCollection},
    cranelift_frontend::{FunctionBuilder, FunctionBuilderContext},
    cranelift_module::{Backend, FuncId, Linkage, Module as CraneliftModule, ModuleError},
};

pub type Product = <FaerieBackend as Backend>::Product;
pub type Module = CraneliftModule<FaerieBackend>;

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

    pub fn compile<E, F>(cb: F) -> Result<Product, E>
    where
        E: Into<std::io::Error>,
        F: FnOnce(&mut FunctionBuilder) -> Result<(), E>,
    {
        let mut compiler = Self::new();

        // Handles setup and teardown logic, i.e.:
        //  - defnining a main and its signature.
        //  - creating a FunctionBuilder for main.
        //  - allocating a stack slot for the exit status code.
        //  - sealing and finalizing of blocks and builder.
        //  - function verification and product creation.

        // Prologue
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

        // Invoke the callback for the source codegen.
        cb(&mut builder)?;

        // Epilogue
        let end = builder.create_ebb();
        builder.ins().jump(end, &[]);
        builder.switch_to_block(end);

        let exit_status = builder.ins().stack_load(I32, exit_status_slot, 0);
        builder.ins().return_(&[exit_status]);

        builder.seal_all_blocks();
        builder.finalize();

        {
            dbg!(&main);
        }

        verify_function(&main, &flags).unwrap();

        compiler
            .define_function(main, "main", Linkage::Export, signature)
            .unwrap();

        Ok(compiler.module.finish())
    }

    pub fn define_function(
        &mut self,
        func: Function,
        name: &str,
        linkage: Linkage,
        signature: Signature,
    ) -> Result<FuncId, ModuleError> {
        let fid = self.module.declare_function(name, linkage, &signature)?;
        let mut ctx = Context::for_function(func);
        self.module.define_function(fid, &mut ctx).unwrap();
        Ok(fid)
    }
}
