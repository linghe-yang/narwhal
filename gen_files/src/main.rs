use clap::{crate_name, crate_version, App, AppSettings, SubCommand};
use crate::gen_breeze_crs::generate_crs;

mod gen_breeze_crs;

fn main() {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .about("A crate that generates files")
        .subcommand(
            SubCommand::with_name("generate_crs")
                .about("Generate CRS with specified faults")
                .args_from_usage("--faults=[NUMBER] 'Sets the number of faults [default: 1]'")
        )
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches();

    // 处理匹配结果
    if let Some(sub_matches) = matches.subcommand_matches("generate_crs") {
        let faults = sub_matches
            .value_of("faults")
            .unwrap_or("1")  // 因为有默认值，不会失败
            .parse::<usize>()
            .expect("Faults must be a valid number");
        
        generate_crs(faults);
    }
}
