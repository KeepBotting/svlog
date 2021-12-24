extern crate sys_info;

use chrono::NaiveDateTime;
use clap::{load_yaml, App};
use glob::glob;
use std::collections::BTreeSet;
use std::convert::TryInto;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};
use sys_info::boottime;

static LOG_DIR: &str = "/var/log/socklog/";
// TODO: find out why there are only 5 digits at the end of socklog timestamps
static DATE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.6f";
static GLOB_ALL_FILES: &[&str] = &["/current", "/*.[su]"];
static GLOB_CURRENT_FILES: &[&str] = &["/current"];

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct LogLine {
    date: NaiveDateTime,
    date_str: String,
    content: String,
}

fn create_logline(line: String) -> LogLine {
    let date_str: &str = &line[..25];
    let date = NaiveDateTime::parse_from_str(date_str, DATE_FORMAT).unwrap();
    let content_str: &str = &line[26..];
    LogLine {
        date,
        date_str: date_str.to_string(),
        content: content_str.to_string(),
    }
}

fn all_services() -> Vec<String> {
    let mut services = Vec::new();
    let path = Path::new(LOG_DIR);
    for entry in path.read_dir().expect("read_dir call failed").flatten() {
        let p = entry.path();
        let filename = p.file_name().unwrap().to_str().unwrap();
        services.push(filename.to_string());
    }
    services
}

fn list_services() {
    for service in all_services() {
        println!(" - {}", service);
    }
}

fn file_paths(services: &[&str], only_current: bool) -> Vec<PathBuf> {
    let globs = match only_current {
        true => GLOB_CURRENT_FILES,
        false => GLOB_ALL_FILES,
    };
    let mut files = Vec::new();
    for service in services {
        for glob_str_ext in globs {
            let glob_str = String::from(LOG_DIR) + service + glob_str_ext;
            for entry in glob(&glob_str[..])
                .expect("Failed to read glob pattern")
                .flatten()
            {
                files.push(entry);
            }
        }
    }
    files
}

fn extract_loglines(
    file: PathBuf,
    loglines: &mut BTreeSet<LogLine>,
    boottime: Option<NaiveDateTime>,
) {
    let file = File::open(file);
    if let Ok(file) = file {
        let reader = BufReader::new(file);
        for line in reader.lines().flatten() {
            let logline = create_logline(line);
            if match boottime {
                Some(time) => time <= logline.date,
                _ => true,
            } {
                loglines.insert(logline);
            }
        }
    }
}

// TODO: check if the boot time is exact enough
fn boot_time() -> NaiveDateTime {
    let now = SystemTime::now();
    let uptime = boottime().unwrap();

    let duration = Duration::new(uptime.tv_sec.try_into().unwrap(), 0);
    let boottime = now.checked_sub(duration).unwrap();

    let secs = boottime.duration_since(SystemTime::UNIX_EPOCH);
    NaiveDateTime::from_timestamp(secs.unwrap().as_secs().try_into().unwrap(), 0)
}

fn show_logs(services: &[&str], since_boot: bool) {
    let files = file_paths(services, false);

    let mut loglines: BTreeSet<LogLine> = BTreeSet::new();
    let boottime: Option<NaiveDateTime> = match since_boot {
        true => Some(boot_time()),
        _ => None,
    };
    for file in files {
        extract_loglines(file, &mut loglines, boottime);
    }
    for logline in loglines {
        println!("{} {}", logline.date_str, logline.content);
    }
}

fn watch_changes(services: &[&str]) {
    let files = file_paths(services, true);

    let mut cmd: String = String::from("tail -Fq -n0 ");
    for file in files {
        let x = file.to_str();
        cmd = cmd + x.unwrap() + " ";
    }

    cmd += " | uniq"; // TODO: is this necessary?
    Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::inherit())
        .output()
        .expect("failed to execute process");
}

fn read_services(services: Option<clap::Values>) -> Vec<&str> {
    if let Some(services) = services {
        let wanted_services: Vec<&str> = services.collect();
        let all_services = all_services();
        wanted_services.iter().all(|value| {
            all_services.contains(&value.to_string()) || panic!("service \"{}\" not found", value)
        });
        return wanted_services;
    }
    ["**"].to_vec()
}

fn main() {
    let cli = load_yaml!("cli.yaml");
    let args = App::from(cli).get_matches();

    if args.is_present("list") {
        list_services();
        std::process::exit(0);
    }

    let services = read_services(args.values_of("services"));
    show_logs(&services, args.is_present("boot"));
    if args.is_present("follow") {
        watch_changes(&services);
    }
}
