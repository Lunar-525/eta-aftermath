use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use aftermath_world::Simulation;
use aftermath_world::events::{totals, write_csv};
use aftermath_world::world::Account;

mod replay;

const USAGE: &str = "Usage: aftermath-world [--seed U64] [--days 1..=500] [--events NEW_FILE.csv] [--map NEW_FILE.html] [--no-familiarity]";

struct Options {
    seed: u64,
    days: u32,
    events: Option<PathBuf>,
    map: Option<PathBuf>,
    use_familiarity: bool,
}

fn options() -> Result<Option<Options>, String> {
    let mut options = Options {
        seed: 42,
        days: 500,
        events: None,
        map: None,
        use_familiarity: true,
    };
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        let flag = flag.to_str().ok_or("expected a UTF-8 option")?;
        match flag {
            "-h" | "--help" => return Ok(None),
            "--no-familiarity" => {
                options.use_familiarity = false;
                continue;
            }
            "--seed" | "--days" | "--events" | "--map" => {}
            _ => return Err(format!("unknown option {flag}")),
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag {
            "--seed" => {
                options.seed = value
                    .to_str()
                    .and_then(|value| value.parse().ok())
                    .ok_or("--seed requires an unsigned 64-bit integer")?;
            }
            "--days" => {
                options.days = value
                    .to_str()
                    .and_then(|value| value.parse().ok())
                    .filter(|value| (1..=500).contains(value))
                    .ok_or("--days requires an integer from 1 to 500")?;
            }
            "--events" => options.events = Some(value.into()),
            "--map" => options.map = Some(value.into()),
            _ => unreachable!(),
        }
    }
    Ok(Some(options))
}

fn energy(units: u32) -> f64 {
    f64::from(units) * f64::from(Account::Power.measurement().0) / 10.0
}

fn run(options: Options, mut output: impl Write) -> io::Result<()> {
    let mut simulation = Simulation::broken_bridge(options.seed);
    simulation.set_familiarity_enabled(options.use_familiarity);
    let opening = simulation.world().clone();
    let mut map = options
        .map
        .map(|path| replay::Replay::new(path, options.seed, &opening, options.use_familiarity));
    writeln!(output, "硅典-断桥 | 2051 | seed={}", options.seed)?;
    for _ in 0..options.days {
        let report = simulation.advance();
        let world = simulation.world();
        if let Some(map) = &mut map {
            map.push(&report, &simulation);
        }
        writeln!(
            output,
            "day={:03} weather={} season={} alive={} food={} water={} fuel={} wood={} steel={} pump={} food_shortfall={} water_shortfall={} water_requested={} water_quality={} shifts_accepted={} shifts_completed={} shifts_unfilled={} medicine={} medical_capacity={} treated={} waiting_for_care={} kits_prepared={} electricity_generated_kwh={} electricity_used_kwh={} grid_online_kw={} grid_load_kw={} voltage_v={} frequency_hz={:.2} granary_condition={} granary_capacity={} backup_quota_batches={} events={}",
            report.day,
            report.weather.name(),
            report.season.name(),
            report.population,
            world.balance(Account::Food),
            world.balance(Account::Water),
            world.balance(Account::Fuel),
            world.balance(Account::Wood),
            world.balance(Account::Steel),
            world.pump_condition,
            report.food_shortfall,
            report.water_shortfall,
            report.water_requested,
            world.water_quality(),
            report.accepted_shifts,
            report.completed_shifts,
            report.unfilled_shifts,
            world.balance(Account::Medicine),
            report.medical_capacity,
            report.treated,
            report.waiting_for_care,
            report.kits_prepared,
            energy(report.grid.renewable + report.grid.backup),
            energy(report.grid.delivered),
            energy(report.grid.renewable + report.grid.backup),
            energy(report.grid.requested),
            report.grid.voltage,
            f64::from(report.grid.frequency_centihz) / 100.0,
            world.granary_condition,
            world.granary_capacity(),
            report.backup_quota,
            report.events.len(),
        )?;
    }

    writeln!(
        output,
        "\naccount opening produced consumed received sent closing"
    )?;
    for account in Account::ALL {
        let totals = totals(simulation.events(), account);
        writeln!(
            output,
            "{} {} {} {} {} {} {}",
            account.name(),
            opening.balance(account),
            totals.produced,
            totals.consumed,
            totals.received,
            totals.sent,
            simulation.world().balance(account),
        )?;
    }
    if let Some(path) = options.events {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut writer = BufWriter::new(file);
        write_csv(&mut writer, simulation.events())?;
        writer.flush()?;
    }
    if let Some(map) = map {
        map.write(simulation.events())?;
    }
    output.flush()
}

fn main() -> ExitCode {
    match options() {
        Ok(Some(options)) => match run(options, io::stdout().lock()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("aftermath-world: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(None) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
