//! The release run's check of a signed artifact (`scripts/sign-update-artifacts.sh`, decisions
//! D15): `knobas-update-verify <file> <file>.sig` verifies the file with the keys this build
//! trusts and checks that the signature's trusted comment names it. `knobas-update-verify --keys`
//! prints the ids of those keys, one a line, which the script compares with the two committed
//! `.pub` files, so a build that trusts a third key fails before upload.

use std::path::Path;
use std::process::ExitCode;

use knobas_update::keys::Keys;
use knobas_update::verify;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let keys = match Keys::compiled() {
        Ok(keys) => keys,
        Err(error) => {
            eprintln!("knobas-update-verify: {error}");
            return ExitCode::FAILURE;
        }
    };
    match args.as_slice() {
        [flag] if flag == "--keys" => {
            for key in keys.all() {
                println!("{}\t{}", key.id, key.name);
            }
            ExitCode::SUCCESS
        }
        [file, sig] => {
            let file = Path::new(file);
            let check = || -> Result<(), String> {
                if keys.is_empty() {
                    return Err("this build trusts no key: crates/knobas-update/keys holds no primary.pub and spare.pub".to_owned());
                }
                let text =
                    std::fs::read_to_string(sig).map_err(|error| format!("{sig}: {error}"))?;
                let signature = verify::decode(&text).map_err(|error| error.to_string())?;
                let name = file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| format!("{} has no file name", file.display()))?;
                keys.verify_file(file, &signature, name, None)
                    .map_err(|error| error.to_string())
            };
            match check() {
                Ok(()) => {
                    println!("verified {}", file.display());
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("knobas-update-verify: {}: {error}", file.display());
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: knobas-update-verify <file> <file>.sig | --keys");
            ExitCode::from(2)
        }
    }
}
