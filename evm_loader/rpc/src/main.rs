// use std::{collections::HashMap, error::Error};
mod build_info;
mod context;
mod error;
mod handlers;
mod options;
mod rpc;

use crate::build_info::get_build_info;
use context::Context;
use error::NeonRPCError;
use neon_lib::config;
use std::{env, net::SocketAddr, str::FromStr};
use tracing::info;
use tracing_appender::non_blocking::NonBlockingBuilder;

type NeonRPCResult<T> = Result<T, NeonRPCError>;

#[actix_web::main]
async fn main() -> NeonRPCResult<()> {
    let matches = options::parse();

    // initialize tracing
    let (non_blocking, _guard) = NonBlockingBuilder::default()
        .lossy(false)
        .finish(std::io::stdout());

    tracing_subscriber::fmt().with_writer(non_blocking).init();

    let lib_dir = matches.value_of("LIB-DIR").unwrap();
    let libraries = neon_lib_interface::load_libraries(lib_dir)?;

    info!("BUILD INFO: {}", get_build_info());
    info!(
        "LIBRARY DIR: {}, count: {}",
        lib_dir,
        libraries.keys().len(),
    );

    if libraries.keys().len() > 0 {
        info!("=== LIBRARY VERSIONS: =================================================================");
        for library_ver in libraries.keys() {
            info!("Lib version: {}", library_ver);
        }
        info!("=== END LIBRARY VERSIONS ==============================================================");
    }

    // check configs
    let api_config = config::load_api_config_from_enviroment();
    let _ = config::create_from_api_config(&api_config)?;

    let ctx = Context { libraries };
    let rpc = rpc::build_rpc(ctx)?;

    let listener_addr = matches
        .value_of("host")
        .map(std::borrow::ToOwned::to_owned)
        .or_else(|| Some(env::var("NEON_API_LISTENER_ADDR").unwrap_or("0.0.0.0:3100".to_owned())))
        .unwrap();

    let addr = SocketAddr::from_str(listener_addr.as_str())?;

    actix_web::HttpServer::new(move || {
        let rpc = rpc.clone();
        actix_web::App::new().service(
            actix_web::web::service("/")
                .guard(actix_web::guard::Post())
                .finish(rpc.into_web_service()),
        )
    })
    .bind(addr)?
    .run()
    .await?;

    Ok(())
}
