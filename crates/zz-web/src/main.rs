use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    process::ExitCode,
};

use zz_web::{Gateway, GatewayConfig};

fn main() -> ExitCode {
    if let Some(socket) = std::env::var_os(zz_daemon_client::ASKPASS_SOCKET_ENV) {
        let prompt = std::env::args_os().nth(1).unwrap_or_default();
        return zz_daemon_client::run_helper(Path::new(&socket), &prompt.to_string_lossy());
    }
    match serve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[tokio::main]
async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let mut bind = SocketAddr::from((
        [127, 0, 0, 1],
        if zz_protocol::app_identity::DEVELOPMENT {
            8081
        } else {
            8080
        },
    ));
    let mut assets = PathBuf::from(if zz_protocol::app_identity::DEVELOPMENT {
        "clients/web/dist-dev"
    } else {
        "clients/web/dist"
    });
    let mut socket = zz_daemon_client::default_socket_path();
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--bind") => {
                bind = arguments
                    .next()
                    .ok_or("--bind needs a loopback address and port")?
                    .to_str()
                    .ok_or("--bind must be valid UTF-8")?
                    .parse()?;
            }
            Some("--assets") => {
                assets = arguments.next().ok_or("--assets needs a directory")?.into();
            }
            Some("--socket") => {
                socket = arguments.next().ok_or("--socket needs a path")?.into();
            }
            Some("--help" | "-h") => {
                println!(
                    "Usage: zz-web [--assets DIR] [--bind ADDRESS:PORT] [--socket PATH]\n\nDefaults: assets={}, bind={bind}, socket={}\nServe the zz browser client and connect each browser to the existing daemon.\nOnly loopback addresses are supported. Hosts in zz/config, and the ones a page adds, are\nreached from this machine with its own ssh identity; a password or host key is asked in the page.\nOpen the printed link: its token is required, and each start makes a new one.",
                    assets.display(),
                    socket.display()
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {}", argument.to_string_lossy()).into()),
        }
    }
    let (hosts, _) = zz_daemon_client::configured_fleet_hosts()
        .map_err(|error| format!("could not read zz/config: {error}"))?;
    let gateway = Gateway::bind(GatewayConfig {
        bind,
        assets,
        socket,
        hosts,
    })
    .await?;
    println!(
        "{} browser client: {}",
        zz_protocol::app_identity::DISPLAY_NAME,
        gateway.open_url()
    );
    gateway.serve().await?;
    Ok(())
}
