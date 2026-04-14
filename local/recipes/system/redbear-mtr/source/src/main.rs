use anyhow::{bail, Result};
use redbear_traceroute::{
    destination_port, format_reply_suffix, probe, resolve_destination, ProbeReply,
};
use std::env;
use std::net::Ipv4Addr;
use std::time::Duration;

const DEFAULT_CYCLES: usize = 10;
const DEFAULT_MAX_HOPS: u8 = 30;
const DEFAULT_TIMEOUT_MS: u64 = 1_000;
const DEFAULT_BASE_PORT: u16 = 33_434;

#[derive(Default)]
struct HopStats {
    responder: Option<Ipv4Addr>,
    sent: usize,
    received: usize,
    last_ms: Option<f64>,
    total_ms: f64,
    best_ms: Option<f64>,
    worst_ms: Option<f64>,
    note: Option<&'static str>,
}

impl HopStats {
    fn record_reply(&mut self, reply: ProbeReply, rtt_ms: f64) {
        self.responder = Some(reply.hop());
        self.received += 1;
        self.last_ms = Some(rtt_ms);
        self.total_ms += rtt_ms;
        self.best_ms = Some(self.best_ms.map_or(rtt_ms, |best| best.min(rtt_ms)));
        self.worst_ms = Some(self.worst_ms.map_or(rtt_ms, |worst| worst.max(rtt_ms)));
        self.note = format_reply_suffix(reply);
    }

    fn loss_percent(&self) -> f64 {
        if self.sent == 0 {
            0.0
        } else {
            ((self.sent - self.received) as f64 / self.sent as f64) * 100.0
        }
    }

    fn avg_ms(&self) -> Option<f64> {
        if self.received == 0 {
            None
        } else {
            Some(self.total_ms / self.received as f64)
        }
    }
}

struct Options {
    destination: String,
    cycles: usize,
    max_hops: u8,
    timeout: Duration,
    base_port: u16,
}

enum ParseOutcome {
    Help(String),
    Options(Options),
}

fn usage(program: &str) -> String {
    format!("Usage: {program} [-c cycles] [-m max_hops] [-w timeout_ms] [-p base_port] destination\n\nPath measurement tool built on redbear-traceroute. Real probing is available only when built for Redox.")
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
    let program = args.next().unwrap_or_else(|| "redbear-mtr".to_string());

    let mut destination = None;
    let mut cycles = DEFAULT_CYCLES;
    let mut max_hops = DEFAULT_MAX_HOPS;
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    let mut base_port = DEFAULT_BASE_PORT;

    let mut rest = args;
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseOutcome::Help(usage(&program))),
            "-c" | "--cycles" => cycles = parse_value(&mut rest, arg.as_str())?,
            "-m" | "--max-hops" => max_hops = parse_value(&mut rest, arg.as_str())?,
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
    if cycles == 0 {
        bail!("cycles must be at least 1");
    }
    if max_hops == 0 {
        bail!("max_hops must be at least 1");
    }

    Ok(ParseOutcome::Options(Options {
        destination,
        cycles,
        max_hops,
        timeout: Duration::from_millis(timeout_ms),
        base_port,
    }))
}

fn print_metric(value: Option<f64>) {
    match value {
        Some(value) => print!("{:>7.1}", value),
        None => print!("{:>7}", "*"),
    }
}

fn run() -> Result<()> {
    let options = match parse_args()? {
        ParseOutcome::Help(help) => {
            println!("{help}");
            return Ok(());
        }
        ParseOutcome::Options(options) => options,
    };
    let destination = resolve_destination(&options.destination)?;
    let mut hops = (0..usize::from(options.max_hops))
        .map(|_| HopStats::default())
        .collect::<Vec<_>>();

    for cycle in 0..options.cycles {
        for ttl in 1..=options.max_hops {
            let hop = &mut hops[usize::from(ttl - 1)];
            hop.sent += 1;

            let sequence = cycle * usize::from(options.max_hops) + usize::from(ttl - 1);
            let dest_port = destination_port(options.base_port, sequence)?;
            let observation = probe(destination, ttl, dest_port, options.timeout)?;

            if let Some(reply) = observation.reply {
                hop.record_reply(reply, observation.rtt.as_secs_f64() * 1_000.0);
                if reply.status != redbear_traceroute::ProbeStatus::Hop {
                    break;
                }
            }
        }
    }

    let last_hop = hops
        .iter()
        .rposition(|hop| hop.sent > 0)
        .map(|idx| idx + 1)
        .unwrap_or(0);

    println!(
        "mtr report to {} ({}), {} cycles",
        options.destination, destination, options.cycles
    );
    println!(
        "{:>3}  {:<15} {:>6} {:>5} {:>7} {:>7} {:>7} {:>7}  Note",
        "Hop", "Host", "Loss%", "Snt", "Last", "Avg", "Best", "Wrst"
    );

    for (index, hop) in hops.into_iter().take(last_hop).enumerate() {
        let host = hop
            .responder
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "???".to_string());
        print!(
            "{:>3}. {:<15} {:>5.1}% {:>5}",
            index + 1,
            host,
            hop.loss_percent(),
            hop.sent
        );
        print_metric(hop.last_ms);
        print_metric(hop.avg_ms());
        print_metric(hop.best_ms);
        print_metric(hop.worst_ms);
        println!("  {}", hop.note.unwrap_or(""));
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-mtr: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::HopStats;

    #[test]
    fn loss_percent_is_zero_without_probes() {
        let stats = HopStats::default();
        assert_eq!(stats.loss_percent(), 0.0);
        assert_eq!(stats.avg_ms(), None);
    }

    #[test]
    fn avg_and_loss_track_sent_and_received_counts() {
        let mut stats = HopStats::default();
        stats.sent = 4;
        stats.received = 3;
        stats.total_ms = 60.0;

        assert!((stats.loss_percent() - 25.0).abs() < f64::EPSILON);
        assert_eq!(stats.avg_ms(), Some(20.0));
    }
}
