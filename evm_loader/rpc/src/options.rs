use clap::ArgMatches;

pub fn parse<'a>() -> ArgMatches<'a> {
    clap::App::new("Neon Core RPC")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Neon Labs")
        .about("Runs a Neon Core RPC server")
        .arg(
            clap::Arg::with_name("LIB-DIR")
                .env("NEON_LIB_DIR")
                .alias("dir")
                .help("Directory with neon libraries to load")
                .required(true)
                .index(1),
        )
        .arg(
            clap::Arg::with_name("HOST")
                .alias("host")
                .env("NEON_API_LISTENER_ADDR")
                .default_value("0.0.0.0:3100")
                .help("RPC host to connect to")
                .required(false)
                .index(2),
        )
        .get_matches()
}
