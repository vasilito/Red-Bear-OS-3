use anyhow::{bail, Context, Result};
use redbear_traceroute::{
    destination_port, format_reply_suffix, probe, resolve_destination, ProbeStatus,
};
use std::env;
use std::time::Duration;

const DEFAULT_MAX_HOPS: u8 = 30;
const DEFAULT_QUERIES: usize = 3;
const DEFAULT_TIMEOUT_MS: u64 = 1_000;
const DEFAULT_BASE_PORT: u16 = 33_434;

struct Options {
    destination: String,
    max_hops: u8,
    queries: usize,
    timeout: Duration,
    base_port: u16,
}

enum ParseOutcome {
    Help(String),
    Options(Options),
}

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} [-m max_hops] [-q queries] [-w timeout_ms] [-p base_port] destination\n\nUDP-based traceroute for Red Bear OS. Real probing is available only when built for Redox."
    )
}

fn parse_value<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    let value = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))?;
    value
        .parse::<T>()
        .map_err(|err| anyhow::anyhow!("invalid value for {flag}: {err}"))
}

fn parse_args() -> Result<ParseOutcome> {
    let mut args = env::args();
    let program = args
        .next()
        .unwrap_or_else(|| "redbear-traceroute".to_string());

    let mut destination = None;
    let mut max_hops = DEFAULT_MAX_HOPS;
    let mut queries = DEFAULT_QUERIES;
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    let mut base_port = DEFAULT_BASE_PORT;

    let mut rest = args;
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseOutcome::Help(usage(&program))),
            "-m" | "--max-hops" => max_hops = parse_value(&mut rest, arg.as_str())?,
            "-q" | "--queries" => queries = parse_value(&mut rest, arg.as_str())?,
            "-w" | "--timeout-ms" => timeout_ms = parse_value(&mut rest, arg.as_str())?,
            "-p" | "--base-port" => base_port = parse_value(&mut rest, arg.as_str())?,
            value if value.starts_with('-') => bail!("unknown option {value}\n{}", usage(&program)),
            value => {
                if destination.replace(value.to_string()).is_some() {
                    bail!("only one destination may be supplied\n{}", usage(&program));
                }
            }
        }
    }

    let destination = destination.ok_or_else(|| anyhow::anyhow!(usage(&program)))?;
    if max_hops == 0 {
        bail!("max_hops must be at least 1");
    }
    if queries == 0 {
        bail!("queries must be at least 1");
    }

    Ok(ParseOutcome::Options(Options {
        destination,
        max_hops,
        queries,
        timeout: Duration::from_millis(timeout_ms),
        base_port,
    }))
}

fn run() -> Result<()> {
    let options = match parse_args()? {
        ParseOutcome::Help(help) => {
            println!("{help}");
            return Ok(());
        }
        ParseOutcome::Options(options) => options,
    };
    let destination = resolve_destination(&options.destination)
        .with_context(|| format!("failed to resolve {}", options.destination))?;

    println!(
        "traceroute to {} ({}), {} hops max",
        options.destination, destination, options.max_hops
    );

    for ttl in 1..=options.max_hops {
        print!("{:>2}", ttl);
        let mut stop = false;

        for query in 0..options.queries {
            let sequence = (usize::from(ttl) - 1) * options.queries + query;
            let dest_port = destination_port(options.base_port, sequence)?;
            let observation = probe(destination, ttl, dest_port, options.timeout)?;

            match observation.reply {
                Some(reply) => {
                    let rtt_ms = observation.rtt.as_secs_f64() * 1_000.0;
                    print!("  {}  {:.1}ms", reply.hop(), rtt_ms);
                    if let Some(suffix) = format_reply_suffix(reply) {
                        print!(" {suffix}");
                    }
                    if matches!(
                        reply.status,
                        ProbeStatus::Reached | ProbeStatus::Unreachable
                    ) {
                        stop = true;
                    }
                }
                None => print!("  *"),
            }
        }

        println!();
        if stop {
            break;
        }
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-traceroute: {err}");
        std::process::exit(1);
    }
}
