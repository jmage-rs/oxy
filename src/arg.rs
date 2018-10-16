//! This module processes command line arguments. Uses [Clap](::clap).
use clap::{App, Arg, SubCommand};

/// Get the Clap app describing the command line arguments accepted by Oxy.
pub fn create_app() -> App<'static, 'static> {
    App::new("oxy")
        .version(::clap::crate_version!())
        .author(::clap::crate_authors!())
        .setting(::clap::AppSettings::SubcommandRequired)
        .subcommand(
            SubCommand::with_name("server")
                .about("Run a server.")
                .arg(Arg::with_name("port").long("port").takes_value(true))
                .arg(
                    Arg::with_name("outer key")
                        .long("outer-key")
                        .takes_value(true)
                        .help("Base32 outer key"),
                ),
        )
        .subcommand(
            SubCommand::with_name("client")
                .about("Connect to a server.")
                .arg(Arg::with_name("destination"))
                .arg(
                    Arg::with_name("outer key")
                        .long("outer-key")
                        .takes_value(true)
                        .help("Base32 outer key"),
                ),
        )
        .subcommand(
            SubCommand::with_name("raw")
                .about("Run a configuration file directly")
                .arg(Arg::with_name("filename").required(true)),
        )
        .subcommand(
            SubCommand::with_name("config")
                .about("Configure Oxy")
                .setting(::clap::AppSettings::SubcommandRequired)
                .subcommand(SubCommand::with_name("init-server")),
        )
}

/// Generate a config data-structure based on command line arguments. Will load
/// and integrate on-disk config files if specified by the arguments.
pub fn args_to_config<T>(args: &[&T]) -> Result<crate::config::Config, ::clap::Error>
where
    T: AsRef<str> + ?Sized,
{
    create_app()
        .get_matches_from_safe(args.iter().map(|x| x.as_ref()))
        .map(config_from_matches)
}

/// Like args_to_config, except handles args that do not meaningfully yield a
/// config and exits after processing if there's no config to be had.
pub fn execute_args<T>(args: &[&T]) -> Result<crate::config::Config, ::clap::Error>
where
    T: AsRef<str> + ?Sized,
{
    let matches = create_app().get_matches_from_safe(args.iter().map(|x| x.as_ref()));
    if matches.is_err() {
        return Err(matches.unwrap_err());
    }
    let matches = matches.unwrap();
    match matches.subcommand() {
        ("config", matches2) => {
            crate::config_wizard::do_wizard(matches2.unwrap());
            ::std::process::exit(0);
        }
        _ => (),
    }
    Ok(config_from_matches(matches))
}

fn config_from_matches(matches: ::clap::ArgMatches) -> crate::config::Config {
    let mut config: crate::config::Config = Default::default();
    match matches.subcommand() {
        ("server", matches2) => {
            let matches2 = matches2.expect("impossible");
            config.mode = Some(crate::config::Mode::Server);
            if let Some(outer_key) = matches2.value_of("outer key") {
                config.outer_key = Some(
                    ::data_encoding::BASE32_NOPAD
                        .decode(outer_key.as_bytes())
                        .expect("invalid outer key cli argument"),
                );
            }
        }
        ("client", matches2) => {
            let matches2 = matches2.expect("impossible");
            config.mode = Some(crate::config::Mode::Client);
            if let Some(destination) = matches2.value_of("destination") {
                config.destination = Some(destination.to_string());
            }
            if let Some(outer_key) = matches2.value_of("outer key") {
                config.outer_key = Some(
                    ::data_encoding::BASE32_NOPAD
                        .decode(outer_key.as_bytes())
                        .expect("invalid outer key cli argument"),
                );
            }
        }
        ("raw", Some(matches2)) => {
            let mut buf = Vec::new();
            let mut file = ::std::fs::File::open(matches2.value_of("filename").unwrap()).unwrap();
            ::std::io::Read::read_to_end(&mut file, &mut buf).unwrap();
            return ::toml::from_slice(&buf).unwrap();
        }
        _ => (),
    }
    config
}
