use {
    cranelift_codegen::{
        ir::{Function, Signature},
        isa::{self},
        settings::{self, Configurable},
        Context,
    },
    cranelift_faerie::{FaerieBackend, FaerieBuilder, FaerieTrapCollection},
    cranelift_module::{Backend, Linkage, Module as CraneliftModule},
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
