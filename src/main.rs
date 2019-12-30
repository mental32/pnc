use {
    pest::Parser,
    pnc::{codegen, Compiler, Penance, Rule},
    std::{
        fs::File,
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
}

fn main() -> io::Result<()> {
    let opts = Opts::from_args();

    let mut buf = String::new();
    File::open(opts.input)?.read_to_string(&mut buf)?;

    if let Ok(stream) = Penance::parse(Rule::file, &buf) {
        let parsed = stream.last().unwrap();

        let product = Compiler::compile(|mut builder| codegen(parsed, &mut builder))?;

        let obj_file = "a.obj";
        let output = "a.out";

        // Assemble and produe object binaries.
        let bytes = product.emit().unwrap();
        File::create(Path::new(obj_file))?.write_all(&bytes)?;

        // Link the object file using host linker
        Command::new("cc")
            .args(&[&Path::new(obj_file), Path::new("-o"), Path::new(output)])
            .status()?;
    } else {
        eprintln!("Failed to parse input.");
        exit(1);
    }

    Ok(())
}
