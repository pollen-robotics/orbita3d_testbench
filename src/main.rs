use orbita3d_controller::Orbita3dController;

use serde::Deserialize;
use serde::Serialize;

use std::time::SystemTime;
use std::{error::Error, thread, time::Duration};

use clap::Parser;

use poulpe_ethercat_grpc::server::launch_server;

/// Orbita3d testbench program
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Orbita3d configuration file
    #[arg(short, long, default_value = "config/fake.yaml")]
    configfile: String,

    /// Should we start the grpc_server to run the program in standalone
    #[arg(short, long)]
    start_server: bool,

    /// Input csv with motion to follow
    #[arg(short, long)]
    input_csv: Option<String>,

    /// Result output csv
    #[arg(short, long)]
    output_csv: Option<String>,

    /// Should we start the live viewer
    #[arg(short, long)]
    viewer: bool,

    /// How many loop should we perform
    #[arg(short, long, default_value = "1")]
    nb_loop: u16,
}

#[derive(Debug, Deserialize)]
// #[serde(rename_all = "PascalCase")]
struct Input {
    timestamp: f64,
    torque_on: bool,
    target_roll: f64,
    target_pitch: f64,
    target_yaw: f64,
    velocity_limit_top: f64,
    velocity_limit_middle: f64,
    velocity_limit_bottom: f64,
    torque_limit_top: f64,
    torque_limit_middle: f64,
    torque_limit_bottom: f64,
}

#[derive(Debug, Serialize)]
struct Output {
    timestamp: f64,
    torque_on: bool,
    present_roll: f64,
    present_pitch: f64,
    present_yaw: f64,
    target_roll: f64,
    target_pitch: f64,
    target_yaw: f64,
    present_pos_top: f64,
    present_pos_mid: f64,
    present_pos_bot: f64,
    present_velocity_top: f64,
    present_velocity_mid: f64,
    present_velocity_bot: f64,
    present_torque_top: f64,
    present_torque_mid: f64,
    present_torque_bot: f64,
    present_temperature_top: f64,
    present_temperature_mid: f64,
    present_temperature_bot: f64,
    axis_sensor_top: f64,
    axis_sensor_mid: f64,
    axis_sensor_bot: f64,
    axis_zeros_top: f64,
    axis_zeros_mid: f64,
    axis_zeros_bot: f64,
    board_temperature_top: f64,
    board_temperature_mid: f64,
    board_temperature_bot: f64,
    board_state: u8,
}

use rprompt::prompt_reply;

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args = Args::parse();

    let rec = if args.viewer {
        let _rec = rerun::RecordingStreamBuilder::new("Test Orbita3d").spawn()?;
        Some(_rec)
    } else {
        None
    };

    if args.start_server {
        log::info!("Starting the server");
        // run in a thread, do not block main thread
        thread::spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .enable_all()
                .build()
                .unwrap()
                .block_on(launch_server("config/ethercat.yaml"))
                .unwrap();
        });
        thread::sleep(Duration::from_secs(2));
    }

    log::info!("Config file: {}", args.configfile);

    let infile_path = match args.input_csv {
        Some(s) => {
            log::info!("Input csv file: {:?}", s);
            s
        }
        None => {
            log::warn!("No input csv file provided");
            let buffer =
                prompt_reply("Please enter the input csv file path [input.csv]: ").unwrap();
            if buffer.trim().is_empty() {
                "input.csv".to_string()
            } else {
                buffer.trim().to_string()
            }
        }
    };

    let outfile_path = match args.output_csv {
        Some(s) => s,
        None => {
            log::warn!("No output csv file provided");
            let buffer =
                prompt_reply("Please enter the output csv file path [output.csv]: ").unwrap();
            if buffer.trim().is_empty() {
                "output.csv".to_string()
            } else {
                buffer.trim().to_string()
            }
        }
    };

    let mut controller = Orbita3dController::with_config(&args.configfile)?;
    let t = controller.is_torque_on();
    match t {
        Ok(t) => log::info!("Torque is {}", t),
        Err(e) => log::error!("Error: {}", e),
    }
    let t = controller.disable_torque(); //Start with torque_off
    match t {
        Ok(_) => {}
        Err(e) => log::error!("Error: {}", e),
    }

    thread::sleep(Duration::from_millis(1000));

    let mut iteration: u16 = 1;
    // let mut input_csv = csv::Reader::from_reader(infile);
    // let startpos = input_csv.position().clone();
    let mut pos = csv::Position::new();

    {
        let infile = match std::fs::File::open(&infile_path) {
            Ok(f) => f,
            Err(e) => {
                log::error!("Error opening input csv file: {}", e);
                return Err(e.into());
            }
        };
        let input_csv = csv::Reader::from_reader(infile);
        // let mut startpos = pos.set_line(2_u64);
        // let mut startpos = pos.set_record(2_u64);
        // let mut startpos = pos.set_byte(218);
        let mut iter = input_csv.into_records();

        for _ in 0..2 {
            // horrible trick to get the position of the first data for later rewind
            pos = iter.reader().position().clone();
            iter.next();
        }
    }
    let startpos = pos;
    let infile = match std::fs::File::open(&infile_path) {
        Ok(f) => f,
        Err(e) => {
            log::error!("Error opening input csv file: {}", e);
            return Err(e.into());
        }
    };
    let mut input_csv = csv::Reader::from_reader(infile);
    while iteration < args.nb_loop + 1 {
        let now = SystemTime::now();
        log::info!("Iteration: {iteration}/{:?}", args.nb_loop);
        let mut all_data: Vec<Output> = Vec::new();

        for in_csv in input_csv.deserialize() {
            let t = now.elapsed().unwrap().as_secs_f64();
            let input_csv_data: Input = in_csv?;
            log::debug!("INPUT: {:?}", input_csv_data);

            //Read feedback from Orbita
            let curr_rpy = controller.get_current_rpy_orientation()?;
            let torque = controller.is_torque_on()?;
            // let curr_vel = controller.get_current_velocity()?;
            // let curr_torque = controller.get_current_torque()?;
            let curr_vel = controller.get_raw_motors_velocity()?;
            let curr_torque = controller.get_raw_motors_current()?;
            let curr_pos = controller.get_raw_motors_positions()?;
            let curr_temp = controller.get_motor_temperatures()?;
            let curr_axis = controller.get_axis_sensors()?;
            let curr_state = controller.get_board_state()?;
            let axis_zeros = controller.get_axis_sensor_zeros()?;
            let board_temp = controller.get_board_temperatures()?;
            all_data.push(Output {
                timestamp: t,
                torque_on: torque,
                present_roll: curr_rpy[0],
                present_pitch: curr_rpy[1],
                present_yaw: curr_rpy[2],
                target_roll: input_csv_data.target_roll,
                target_pitch: input_csv_data.target_pitch,
                target_yaw: input_csv_data.target_yaw,
                present_pos_top: curr_pos[0],
                present_pos_mid: curr_pos[1],
                present_pos_bot: curr_pos[2],
                present_velocity_top: curr_vel[0],
                present_velocity_mid: curr_vel[1],
                present_velocity_bot: curr_vel[2],
                present_torque_top: curr_torque[0],
                present_torque_mid: curr_torque[1],
                present_torque_bot: curr_torque[2],
                present_temperature_top: curr_temp[0],
                present_temperature_mid: curr_temp[1],
                present_temperature_bot: curr_temp[2],
                axis_sensor_top: curr_axis[0],
                axis_sensor_bot: curr_axis[2],
                axis_sensor_mid: curr_axis[1],
                axis_zeros_top: axis_zeros[0],
                axis_zeros_mid: axis_zeros[2],
                axis_zeros_bot: axis_zeros[1],
                board_temperature_top: board_temp[0],
                board_temperature_mid: board_temp[1],
                board_temperature_bot: board_temp[2],
                board_state: curr_state,
            });

            let tosleep = (input_csv_data.timestamp - t) * 1000.0;
            thread::sleep(Duration::from_millis(tosleep as u64));

            //Write commands to Orbita
            if input_csv_data.torque_on {
                controller.enable_torque(true)?;
            } else {
                controller.disable_torque()?;
            }
            controller.set_target_rpy_orientation([
                input_csv_data.target_roll,
                input_csv_data.target_pitch,
                input_csv_data.target_yaw,
            ])?;

            controller.set_raw_motors_velocity_limit([
                input_csv_data.velocity_limit_top,
                input_csv_data.velocity_limit_middle,
                input_csv_data.velocity_limit_bottom,
            ])?;

            controller.set_raw_motors_torque_limit([
                input_csv_data.torque_limit_top,
                input_csv_data.torque_limit_middle,
                input_csv_data.torque_limit_bottom,
            ])?;

            // Rerun
            if let Some(rec) = &rec {
                rec.set_time_seconds("timestamp", t);
                rec.log(
                    "target/torque_on",
                    &rerun::Scalar::new(if input_csv_data.torque_on { 1.0 } else { 0.0 }),
                )?;
                rec.log("target/board_state", &rerun::Scalar::new(curr_state as f64))?;

                rec.log(
                    "position/target/roll",
                    &rerun::Scalar::new(input_csv_data.target_roll),
                )?;
                rec.log(
                    "position/target/pitch",
                    &rerun::Scalar::new(input_csv_data.target_pitch),
                )?;
                rec.log(
                    "position/target/yaw",
                    &rerun::Scalar::new(input_csv_data.target_yaw),
                )?;

                rec.log("position/present/roll", &rerun::Scalar::new(curr_rpy[0]))?;
                rec.log("position/present/pitch", &rerun::Scalar::new(curr_rpy[1]))?;
                rec.log("position/present/yaw", &rerun::Scalar::new(curr_rpy[2]))?;

                rec.log("velocity/present/roll", &rerun::Scalar::new(curr_vel[0]))?;
                rec.log("velocity/present/pitch", &rerun::Scalar::new(curr_vel[1]))?;
                rec.log("velocity/present/yaw", &rerun::Scalar::new(curr_vel[2]))?;

                rec.log("torque/present/roll", &rerun::Scalar::new(curr_torque[0]))?;
                rec.log("torque/present/pitch", &rerun::Scalar::new(curr_torque[1]))?;
                rec.log("torque/present/yaw", &rerun::Scalar::new(curr_torque[2]))?;

                rec.log(
                    "position/axis_sensor/roll",
                    &rerun::Scalar::new(curr_axis[0]),
                )?;
                rec.log(
                    "position/axis_sensor/pitch",
                    &rerun::Scalar::new(curr_axis[1]),
                )?;
                rec.log(
                    "position/axis_sensor/yaw",
                    &rerun::Scalar::new(curr_axis[2]),
                )?;

                rec.log(
                    "limits/velocity/top",
                    &rerun::Scalar::new(input_csv_data.velocity_limit_top),
                )?;
                rec.log(
                    "limits/velocity/middle",
                    &rerun::Scalar::new(input_csv_data.velocity_limit_middle),
                )?;
                rec.log(
                    "limits/velocity/bottom",
                    &rerun::Scalar::new(input_csv_data.velocity_limit_bottom),
                )?;

                rec.log(
                    "limits/torque/top",
                    &rerun::Scalar::new(input_csv_data.torque_limit_top),
                )?;
                rec.log(
                    "limits/torque/middle",
                    &rerun::Scalar::new(input_csv_data.torque_limit_middle),
                )?;
                rec.log(
                    "limits/torque/bottom",
                    &rerun::Scalar::new(input_csv_data.torque_limit_bottom),
                )?;

                rec.log("temperature/motor/top", &rerun::Scalar::new(curr_temp[0]))?;
                rec.log("temperature/motor/mod", &rerun::Scalar::new(curr_temp[1]))?;
                rec.log("temperature/motor/bot", &rerun::Scalar::new(curr_temp[2]))?;

                rec.log("temperature/board/top", &rerun::Scalar::new(board_temp[0]))?;
                rec.log("temperature/board/mod", &rerun::Scalar::new(board_temp[1]))?;
                rec.log("temperature/board/bot", &rerun::Scalar::new(board_temp[2]))?;
            }
        }

        let torque = controller.disable_torque();
        match torque {
            Ok(_) => log::info!("Torque is off"),
            Err(e) => log::error!("Error: {}", e),
        }
        thread::sleep(Duration::from_millis(1000));

        if args.nb_loop > 1 {
            let outfile_it = format!("{outfile_path}_{iteration}");
            log::info!("Writing output csv file: {}", outfile_it);
            let outfile = match std::fs::File::create_new(&outfile_it) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Error opening output csv file: {}", e);
                    return Err(e.into());
                }
            };
            let mut output_csv = csv::Writer::from_writer(outfile);
            for data in all_data {
                output_csv.serialize(data)?;
            }
        } else {
            log::info!("Writing output csv file: {}", outfile_path);
            let outfile = match std::fs::File::create_new(&outfile_path) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Error opening output csv file: {}", e);
                    return Err(e.into());
                }
            };
            let mut output_csv = csv::Writer::from_writer(outfile);
            for data in all_data {
                output_csv.serialize(data)?;
            }
        }

        iteration += 1;
        input_csv.seek(startpos.clone())?;
    }

    Ok(())
}
