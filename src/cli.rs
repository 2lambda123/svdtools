use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use svdtools::{
    analyze::analyze_cli::{self, CompareConfig},
    convert::convert_cli,
    interrupts::interrupts_cli,
    makedeps::makedeps_cli,
    mmap::mmap_cli,
    patch::patch_cli,
};

#[derive(Parser, Debug)]
#[clap(about, version, author)]
enum Command {
    /// Lints for SVD
    Analyze {
        /// Path to input file
        in_path: PathBuf,

        /// Format of input file (XML, JSON or YAML)
        #[clap(long = "input-format")]
        input_format: Option<convert_cli::InputFormat>,

        /// Items are equal if descriptions the same
        #[clap(long)]
        compare_description: bool,

        /// Compare fields
        #[clap(long)]
        with_fields: bool,
    },
    /// Patches an SVD file as specified by a YAML file
    Patch {
        /// Path to input YAML file
        yaml_file: PathBuf,

        /// Path to output file. By default it just adds `.patched` at the end
        out_path: Option<PathBuf>,

        /// Path to format config file
        ///
        /// If not specified, the default format config will be used.
        #[clap(long)]
        format_config: Option<PathBuf>,
    },
    /// Generate Make dependency file listing dependencies for a YAML file.
    Makedeps {
        /// Input yaml file
        yaml_file: PathBuf,

        /// Dependencies output file
        deps_file: PathBuf,
    },
    /// Print list of all interrupts described by an SVD file
    Interrupts {
        /// Path to input SVD file
        svd_file: PathBuf,

        /// Whether to print gaps in interrupt number sequence
        #[clap(long)]
        no_gaps: bool,
    },
    /// Generate text-based memory map of an SVD file.
    Mmap {
        /// Path to input SVD file
        svd_file: PathBuf,
    },
    /// Convert SVD representation between file formats
    Convert {
        /// Path to input file
        in_path: PathBuf,

        /// Path to output file
        out_path: PathBuf,

        /// Format of input file (XML, JSON or YAML)
        #[clap(long = "input-format")]
        input_format: Option<convert_cli::InputFormat>,

        /// Format of output file (XML, JSON or YAML)
        #[clap(long = "output-format")]
        output_format: Option<convert_cli::OutputFormat>,

        /// Expand arrays, clusters and derived values
        #[clap(long)]
        expand: bool,

        /// Take size, access, reset_value, reset_mask from parents if absent in register
        #[clap(long)]
        expand_properties: bool,

        /// Skip enumeratedValues and writeConstraints during parsing (XML input only)
        #[clap(long)]
        ignore_enums: bool,

        /// Path to format config file
        ///
        /// If not specified, the default format config will be used.
        ///
        /// Only used for SVD output format.
        #[clap(long)]
        format_config: Option<PathBuf>,
    },
}

impl Command {
    pub fn run(&self) -> Result<()> {
        match self {
            Self::Analyze {
                in_path,
                input_format,
                compare_description,
                with_fields,
            } => {
                analyze_cli::analyze_file(
                    in_path,
                    *input_format,
                    &CompareConfig {
                        compare_description: *compare_description,
                        with_fields: *with_fields,
                    },
                )?;
            }
            Self::Interrupts { svd_file, no_gaps } => {
                interrupts_cli::parse_device(svd_file, !no_gaps)?;
            }
            Self::Mmap { svd_file } => mmap_cli::parse_device(svd_file)?,
            Self::Patch {
                yaml_file,
                out_path,
                format_config,
            } => {
                patch_cli::patch(yaml_file, out_path.as_deref(), format_config.as_deref())?;
            }
            Self::Makedeps {
                yaml_file,
                deps_file,
            } => makedeps_cli::makedeps(yaml_file, deps_file)?,
            Self::Convert {
                in_path,
                out_path,
                input_format,
                output_format,
                expand,
                expand_properties,
                ignore_enums,
                format_config,
            } => convert_cli::convert(
                in_path,
                out_path,
                *input_format,
                *output_format,
                convert_cli::ParserConfig {
                    expand: *expand,
                    expand_properties: *expand_properties,
                    ignore_enums: *ignore_enums,
                },
                format_config.as_ref().map(|p| p.as_path()),
            )?,
        }
        Ok(())
    }
}

#[derive(Parser, Debug)]
struct CliArgs {
    #[clap(subcommand)]
    command: Command,
}

pub fn run() {
    use anyhow::Context;

    env_logger::init();

    let args = CliArgs::parse();
    if let Err(e) = args
        .command
        .run()
        .with_context(|| format!("by svdtools ({})", clap::crate_version!()))
    {
        log::error!("{:?}", e);

        std::process::exit(1);
    }
}
