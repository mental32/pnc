use {
    pest::Parser,
    pnc::{codegen, Compiler, Penance, Rule},
    std::{
        fs::{File, remove_file},
        io::{self, Read, Write},
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

    #[structopt(short, long, default_value = "a.out", parse(from_os_str))]
    output: PathBuf,
}

fn main() -> io::Result<()> {
    let opts = Opts::from_args();

    let mut buf = String::new();
    File::open(opts.input)?.read_to_string(&mut buf)?;

    if let Ok(stream) = Penance::parse(Rule::file, &buf) {
        let parsed = stream.last().unwrap();

        let mut symbol_table = SymbolTable::new();
        let product = Compiler::compile(|mut builder| codegen(parsed, &mut builder, &mut symbol_table))?;

        let object_file = Path::new("a.obj");

        // Assemble and produe object binaries.
        let bytes = product.emit().unwrap();
        File::create(object_file)?.write_all(&bytes)?;

        // Link the object file using host linker
        Command::new("cc")
            .args(&[&object_file, Path::new("-o"), &opts.output])
            .status()?;

        remove_file(object_file)?;
    } else {
        eprintln!("Failed to parse input.");
        exit(1);
    }

    Ok(())
}
