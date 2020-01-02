use cargo::util::Config;
use cargo_flutter::package::appimage::AppImage;
use cargo_flutter::{Build, Cargo, Engine, Error, Flutter, Package, TomlConfig};
use clap::{App, AppSettings, Arg, SubCommand};
use std::{env, str};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let app_matches = App::new("cargo-flutter")
        .bin_name("cargo")
        .subcommand(
            SubCommand::with_name("flutter")
                .setting(AppSettings::TrailingVarArg)
                .version(env!("CARGO_PKG_VERSION"))
                .author("flutter-rs")
                .about("Provides a smooth experience for developing flutter-rs apps.")
                .arg(
                    Arg::with_name("format")
                        .short("f")
                        .long("format")
                        .value_name("FORMAT")
                        .takes_value(true)
                        .help("Packaging format"),
                )
                .arg(
                    Arg::with_name("no-flutter")
                        .long("no-flutter")
                        .help("shortcut for no-bundle, no-attach and no-aot"),
                )
                .arg(
                    Arg::with_name("no-bundle")
                        .long("no-bundle")
                        .help("Skips running flutter bundle"),
                )
                .arg(
                    Arg::with_name("no-attach")
                        .long("no-attach")
                        .help("Skips attaching the flutter debugger"),
                )
                .arg(
                    Arg::with_name("no-aot")
                        .long("no-aot")
                        .help("Skips creating aot blob"),
                )
                .arg(
                    Arg::with_name("sign")
                        .long("sign")
                        .help("Sign package in debug build"),
                )
                .arg(
                    Arg::with_name("no-sign")
                        .long("no-sign")
                        .help("Don't sign package in release build"),
                )
                .arg(
                    Arg::with_name("cargo-args")
                        .value_name("CARGO_ARGS")
                        .takes_value(true)
                        .required(true)
                        .multiple(true),
                ),
        )
        .get_matches();

    let matches = if let Some(matches) = app_matches.subcommand_matches("flutter") {
        matches
    } else {
        return Err(Error::NotCalledWithCargo.into());
    };

    let cargo_args: Vec<&str> = matches
        .values_of("cargo-args")
        .expect("cargo-args to not be null")
        .collect();
    let cargo_config = Config::default()?;
    let cargo = Cargo::new(&cargo_config, cargo_args)?;
    let build = if cargo.release() {
        Build::Release
    } else {
        Build::Debug
    };
    let aot = build == Build::Release;
    let sign = build == Build::Debug && matches.is_present("sign")
        || build == Build::Release && !matches.is_present("no-sign");
    let config = TomlConfig::load(&cargo).ok();
    let metadata = config
        .as_ref()
        .map(|config| config.metadata())
        .unwrap_or_default();
    let flutter = Flutter::new()?;
    let engine_version = metadata.engine_version().unwrap_or_else(|| {
        std::env::var("FLUTTER_ENGINE_VERSION")
            .ok()
            .unwrap_or_else(|| {
                //flutter.engine_version().unwrap()
                Engine::latest_version().unwrap()
            })
    });

    log::debug!("FLUTTER_ROOT {}", flutter.root().display());
    log::debug!("FLUTTER_ENGINE_VERSION {}", engine_version);

    let engine = Engine::new(engine_version, cargo.triple()?, build);
    let engine_path = engine.engine_path();
    let flutter_asset_dir = cargo.build_dir().join("flutter_assets");
    let snapshot_path = cargo.build_dir().join("app.so");

    log::debug!("FLUTTER_ENGINE_PATH {}", engine_path.display());
    log::debug!("FLUTTER_ASSET_DIR {}", flutter_asset_dir.display());

    engine.download();

    if config.is_some() {
        if !matches.is_present("no-flutter") && !matches.is_present("no-bundle") {
            println!("flutter build bundle");
            flutter.bundle(&cargo, build)?;
        }

        if !matches.is_present("no-flutter") && !matches.is_present("no-aot") {
            if aot {
                flutter.aot(&cargo, &engine_path)?;
            }
        }
    }

    match (cargo.cmd(), config) {
        ("build", Some(config)) => {
            cargo.build(&engine_path)?;

            if let Some(format) = matches.value_of("format") {
                let mut package = Package::new(&config.package.name);
                package.add_bin(cargo.build_dir().join(&config.package.name));
                package.add_lib(engine_path);
                if aot {
                    package.add_lib(snapshot_path);
                }
                package.add_asset(flutter_asset_dir);
                match format {
                    "appimage" => {
                        let builder = AppImage::new(metadata.appimage.unwrap_or_default());
                        builder.build(&cargo, &package, sign)?;
                    }
                    _ => Err(Error::FormatNotSupported)?,
                }
            }
        }
        ("run", Some(_config)) => {
            std::env::set_var("FLUTTER_AOT_SNAPSHOT", &snapshot_path);
            std::env::set_var("FLUTTER_ASSET_DIR", &flutter_asset_dir);
            let debug_uri = cargo.run(&engine_path)?;
            log::info!("Observatory at {}", debug_uri);

            if !matches.is_present("no-flutter") && !matches.is_present("no-attach") {
                flutter.attach(&cargo, &debug_uri)?;
            }
        }
        _ => cargo.build(&engine_path)?,
    }

    Ok(())
}
