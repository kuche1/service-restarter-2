
use clap::Parser; // cargo add clap --features derive
use chrono; // cargo add chrono
use chrono::NaiveTime;
use std::process::ExitCode;
use std::thread;
use std::time;
use systemctl; // cargo add chrono
use std::fs;
use std::fs::File;
use std::io::Write;
use lazy_static::lazy_static; // cargo add lazy_static
use std::sync::RwLock;
use online; // cargo add online
use system_shutdown; // cargo add system_shutdown
use std::process::Command;

lazy_static! {
	static ref ERROR_FOLDER: RwLock<String> = RwLock::new("UNRNEACHABLE-error-restarter".to_string());
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {

    /// Folder to write error info to
    #[arg(long)]
    error_folder: String,

	/// When the restart is going to occur, for example 15 for 15:00
    #[arg(short, long, default_value_t = 4)] // uvu helpa to ti kazva `-r, --restart-hour <RESTART_HOUR>   Hour at which the restart will occur` i tova `short` e `-r` a `long` e `--restart-hour`, a puk "komentara" otgore e description-a koito shte izpishe
    restart_at: u8,

    /// Time to sleep if restart time has not been reached
    #[arg(long, default_value_t = 3600)] // not providing short since there is a conflict with `services` (both start with `s`)
    check_time_sleep_sec: u64,

	/// Time to sleep after a service has been restarted, as to give services breathing root
	#[arg(long)]
	service_restarted_sleep_sec: u64,

    /// Services to startart, each needs to end with .service
    #[arg(short, long)]
    services: Vec<String>, // this CAN be empty, multiple services specified with `--services asd --services dfg`
}

fn logerr(msg:String){
	eprintln!("ERROR: {msg}");

	fs::create_dir_all(ERROR_FOLDER.read().unwrap().to_owned()).unwrap();

	let now = chrono::offset::Local::now();
	let file_name = now.format("%Y-%m-%d_%H-%M-%S-%f"); // %f - nanoseconds

	let mut f = File::options()
		.append(true)
		.create(true)
		.open(format!("{}/{}", ERROR_FOLDER.read().unwrap().to_owned(), file_name))
		.unwrap();

	writeln!(&mut f, "{}", msg).unwrap();
}

fn sync(){
	let cmd = Command
		::new("sync")
		.output();

	let cmd = match cmd{
		Ok(val) => val,
		Err(err) => {
			logerr(format!("could not call `sync`: {err}"));
			return;
		},
	};

	if !cmd.status.success(){
		logerr("able to call `sync`; bad return code".to_string());
	}
}

fn main() -> ExitCode {

	let args = Args::parse();

	if args.restart_at >= 24 {
		eprintln!("restart_at cannot be >= 24 (restart_at={})", args.restart_at);
		return ExitCode::FAILURE;
	};

	{
		let mut new_error_folder = ERROR_FOLDER.write().unwrap();
		*new_error_folder = args.error_folder;
	}

	{ // wait for the right time to restart

		let restart_at = args.restart_at;
		let sleep_sec = args.check_time_sleep_sec;

		let target = NaiveTime::from_hms_opt(restart_at.into(), 0, 0).unwrap();

		loop{
			let now = chrono::offset::Local::now().time();

			println!("{target} >? {now}");

			if now > target {
				println!("too late for a restart; sleeping {} sec", sleep_sec);
				thread::sleep(time::Duration::from_secs(sleep_sec));
			}else{
				break;
			}
		}

		loop{
			let now = chrono::offset::Local::now().time();

			println!("{target} <? {now}");

			if now < target {
				println!("too early for a restart; sleeping {} sec", sleep_sec);
				thread::sleep(time::Duration::from_secs(sleep_sec));
			}else{
				break;
			}
		}
	}

	{ // restart whole server if no internet

		if online::check(None).is_err() {

			logerr("no internet; restarting whole server".to_string());

			sync();

			if let Err(err) = system_shutdown::reboot() {
				logerr(format!("could not restart server: {err}"));
			}
		}
	}

	{ // restart

		let restart_sleep_sec = args.service_restarted_sleep_sec;
		let services = args.services;

		let systemctl = systemctl::SystemCtl::default();

		for service in services {

			println!();
			println!("vvv restarting: {}", service);

			// anonymous functions (called closures) use the syntax `|args| -> ret_type code`
			let success = || -> bool {

				// alternatively, we could call `systemctl try-restart <service>`
				// note: if the service is in state "activating" it actually DOES restart it

				if !systemctl.exists(&service).unwrap() {
					logerr(format!("service `{service}` doesn't exist"));
					return false;
				}

				let unit = systemctl.create_unit(&service).unwrap();
				// dbg!(unit);

				// !systemctl.is_active(&service).unwrap() // this actually doesnt seem to consider "activating" services as active

				if !unit.active { // this also doesn't considen an "activating" service as active
					if unit.auto_start != systemctl::AutoStartStatus::Enabled {
						println!("service `{service}` is neither active not enabled -> not restarting");
						return false;
					}
				}

				let exit_status = systemctl.restart(&service).unwrap();
				if !exit_status.success(){
					let return_code = exit_status.code().unwrap();
					logerr(format!("could not restart service `{service}` -> return code {return_code}"));
					return false;
				}

				return true;
			}();

			if success {
				println!("service `{service}` restarted; giving some breating room; sleeping {} sec", restart_sleep_sec);
				thread::sleep(time::Duration::from_secs(restart_sleep_sec));
			}

			println!("^^^ restarting: {}", service);

		}
	}

	// if we get to this point, the restarter service itself has not been restarted
	logerr("unreachable: service restarter should have restarted itself".to_string());
	return ExitCode::FAILURE;

	// return ExitCode::SUCCESS;
}
