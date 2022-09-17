mod artifact;
mod envvar;
mod error;
mod record;
mod run;
use crate::error::Result;

fn help(arg0: &str, msg: Option<&str>) {
    match msg {
        Some(s) => println!("{}", s),
        None => {
            println!("{} run <program> [args]...", arg0);
            println!(" envvars:");
            println!("   LW_TARGET_ID: Identity of target resource");
            println!("   LW_WORK_NAME: My work name");
            println!("   LW_WORK_VERSION: My work version");
            println!(
                "   LW_DEPENDS_xxx:  Dependent work names. The xxx is specific to actual work code."
            );
            println!("   LW_INDIR: ");
            println!("   LW_OUTDIR: ");
            println!("");
            println!("   LW_MONGODB_URI: http[s]://<username>:<password>@<hostname>[:port]");
            println!("   LW_MONGODB_DATABASE: ");
            println!("   LW_MONGODB_COLLECTION: ");
            println!("   LW_MONGODB_URI: ");
            println!("");
            println!("   LW_S3_ACCESS_KEY: ");
            println!("   LW_S3_SECRET_KEY: ");
            println!("   LW_S3_BUCKET: ");
            println!("   LW_S3_REGION: optional.");
            println!("   LW_S3_ENDPOINT: optional.");
            println!("   LW_S3_PATH_STYLE: \"true\" of \"false\". optional, default is \"true\".");
            println!("{} scan", arg0);
        }
    }
    std::process::exit(0);
}

#[async_std::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        help(&args[0], None);
    }
    let r = match args[1].as_str() {
        "run" => crate::run::run_from_env(&args[2..]).await,
        "scan" => Ok(()),
        _ => {
            help(&args[0], Some(&format!("unkown subcommand: {}", args[1])));
            Ok(())
        }
    };
    if let Err(ref e) = r {
        println!("{}", e.to_string());
        //e.backtrace().map(|bt| println!("{}", bt));
    }
    r
}
