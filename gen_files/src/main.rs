use clap::{crate_name, crate_version, App, AppSettings, SubCommand};
use crate::gen_breeze_crs::generate_crs;

mod gen_breeze_crs;

#[cfg(not(feature = "pq"))]
fn main() {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about("A crate that generates files")
        .subcommand(
            SubCommand::with_name("generate_crs")
                .about("Generate CRS with specified faults")
                .args_from_usage("--fault_tolerance=[NUMBER] 'Sets the fault tolerance [default: 1]'")
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    // 处理匹配结果
    if let Some(sub_matches) = matches.subcommand_matches("generate_crs") {
        let faults = sub_matches
            .value_of("fault_tolerance")
            .unwrap_or("1")  // 因为有默认值，不会失败
            .parse::<usize>()
            .expect("Fault tolerance must be a valid number");
        
        #[cfg(not(feature = "pq"))]
        generate_crs(faults);
    }
}

#[cfg(feature = "pq")]
fn main() {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about("A crate that generates files")
        .subcommand(
            SubCommand::with_name("generate_crs")
                .about("Generate CRS with specified parameters")
                .args_from_usage(
                    "--n=[NUMBER] 'Sets the lattice base number [default: 128]'
                     --log_q=[NUMBER] 'Sets the logarithm of modulus [default: 32]'
                     --g=[NUMBER] 'Sets the secret aggregation degree [default: 4]'
                     --kappa=[NUMBER] 'Sets the statistical parameter [default: 128]'
                     --r=[NUMBER] 'Sets the folding factor [default: 4]'
                     --ell=[NUMBER] 'Sets the number of nested G^-1(.) [default: 1]'"
                )
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    // 处理匹配结果
    if let Some(sub_matches) = matches.subcommand_matches("generate_crs") {
        let n = sub_matches
            .value_of("n")
            .unwrap_or("128")
            .parse::<usize>()
            .expect("n must be a valid number");

        let log_q = sub_matches
            .value_of("log_q")
            .unwrap_or("32")
            .parse::<u32>()
            .expect("log_q must be a valid number");

        let g = sub_matches
            .value_of("g")
            .unwrap_or("4")
            .parse::<usize>()
            .expect("g must be a valid number");

        let kappa = sub_matches
            .value_of("kappa")
            .unwrap_or("128")
            .parse::<usize>()
            .expect("kappa must be a valid number");

        let r = sub_matches
            .value_of("r")
            .unwrap_or("4")
            .parse::<usize>()
            .expect("r must be a valid number");

        let ell = sub_matches
            .value_of("ell")
            .unwrap_or("1")
            .parse::<usize>()
            .expect("ell must be a valid number");

        generate_crs(n, log_q, g, kappa, r, ell);
    }
}