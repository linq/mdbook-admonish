use clap::{crate_version, Arg, ArgMatches, Command};
use mdbook::errors::Error;
use mdbook::preprocess::{CmdPreprocessor, Preprocessor};
use mdbook_admonish::Admonish;

use std::{io, process};

pub fn make_app() -> Command<'static> {
    let mut command = Command::new("mdbook-admonish")
        .version(crate_version!())
        .about("mdbook preprocessor to add support for admonitions");
    command = command.subcommand(
        Command::new("supports")
            .arg(Arg::new("renderer").required(true))
            .about("Check whether a renderer is supported by this preprocessor"),
    );

    #[cfg(feature = "cli-install")]
    {
        command = command.subcommand(
        Command::new("install")
            .arg(Arg::new("css-dir").long("css-dir").default_value(".").help(
                "Relative directory for the css assets,\nfrom the book directory root",
            ))
            .arg(Arg::new("dir").default_value(".").help(
                "Root directory for the book,\nshould contain the configuration file (`book.toml`)",
            ))
            .about("Install the required assset files and include it in the config"));
    }
    command
}

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let matches = make_app().get_matches();

    if let Some(sub_args) = matches.subcommand_matches("supports") {
        handle_supports(sub_args);
    } else if let Some(sub_args) = matches.subcommand_matches("install") {
        #[cfg(feature = "cli-install")]
        {
            install::handle_install(sub_args);
        }
        #[cfg(not(feature = "cli-install"))]
        {
            panic!("cli-install feature not enabled: {:?}", sub_args)
        }
    } else if let Err(e) = handle_preprocessing() {
        eprintln!("{}", e);
        process::exit(1);
    }
}

fn handle_preprocessing() -> Result<(), Error> {
    let (ctx, book) = CmdPreprocessor::parse_input(io::stdin())?;

    if ctx.mdbook_version != mdbook::MDBOOK_VERSION {
        eprintln!(
            "Warning: The mdbook-admonish preprocessor was built against version \
             {} of mdbook, but we're being called from version {}",
            mdbook::MDBOOK_VERSION,
            ctx.mdbook_version
        );
    }

    let processed_book = Admonish.run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed_book)?;

    Ok(())
}

fn handle_supports(sub_args: &ArgMatches) -> ! {
    let renderer = sub_args.value_of("renderer").expect("Required argument");
    let supported = Admonish.supports_renderer(renderer);

    // Signal whether the renderer is supported by exiting with 1 or 0.
    if supported {
        process::exit(0);
    } else {
        process::exit(1);
    }
}

#[cfg(feature = "cli-install")]
mod install {
    use clap::ArgMatches;
    use std::{
        fs::{self, File},
        io::Write,
        path::PathBuf,
        process,
    };

    const ADMONISH_CSS_FILES: &[(&str, &[u8])] = &[(
        "mdbook-admonish.css",
        include_bytes!("assets/mdbook-admonish.css"),
    )];

    use toml_edit::{value, Array, Document, Item, Table, Value};
    pub fn handle_install(sub_args: &ArgMatches) -> () {
        let dir = sub_args.value_of("dir").expect("Required argument");
        let css_dir = sub_args.value_of("css-dir").expect("Required argument");
        let proj_dir = PathBuf::from(dir);
        let config = proj_dir.join("book.toml");

        if !config.exists() {
            log::error!("Configuration file '{}' missing", config.display());
            process::exit(1);
        }

        log::info!("Reading configuration file '{}'", config.display());
        let toml = fs::read_to_string(&config).expect("can't read configuration file");
        let mut doc = toml
            .parse::<Document>()
            .expect("configuration is not valid TOML");

        if preprocessor(&mut doc).is_none() {
            log::info!("Unexpected configuration, not updating prereprocessor configuration");
        };

        let mut additional_css = additional_css(&mut doc);
        for (name, content) in ADMONISH_CSS_FILES {
            let filepath = proj_dir.join(css_dir).join(name);
            let filepath_str = filepath.to_str().expect("non-utf8 filepath");

            if let Some(ref mut additional_css) = additional_css {
                if !has_file(additional_css, filepath_str) {
                    log::info!("Adding '{filepath_str}' to 'additional-css'");
                    additional_css.push(filepath_str);
                }
            } else {
                log::warn!("Unexpected configuration, not updating 'additional-css'");
            }

            log::info!(
                "Copying '{name}' to '{filepath}'",
                filepath = filepath.display()
            );
            let mut file = File::create(filepath).expect("can't open file for writing");
            file.write_all(content)
                .expect("can't write content to file");
        }

        let new_toml = doc.to_string();
        if new_toml != toml {
            log::info!("Saving changed configuration to '{}'", config.display());
            let mut file =
                File::create(config).expect("can't open configuration file for writing.");
            file.write_all(new_toml.as_bytes())
                .expect("can't write configuration");
        } else {
            log::info!("Configuration '{}' already up to date", config.display());
        }

        log::info!("mdbook-admonish is now installed. You can start using it in your book.");
        let codeblock = r#"```admonish warning
A beautifully styled message.
```"#;
        log::info!("Add a code block like:\n{}", codeblock);

        process::exit(0);
    }

    fn additional_css<'a>(doc: &'a mut Document) -> Option<&'a mut Array> {
        let doc = doc.as_table_mut();

        let empty_table = Item::Table(Table::default());
        let empty_array = Item::Value(Value::Array(Array::default()));

        let item = doc.entry("output").or_insert(empty_table.clone());
        let item = item.as_table_mut()?.entry("html").or_insert(empty_table);
        item.as_table_mut()?
            .entry("additional-css")
            .or_insert(empty_array)
            .as_value_mut()?
            .as_array_mut()
    }

    // TODO(tommilligan) make error handling nicer
    fn preprocessor(doc: &mut Document) -> Option<()> {
        let doc = doc.as_table_mut();

        let empty_table = Item::Table(Table::default());
        let item = doc.entry("preprocessor").or_insert(empty_table.clone());
        let item = item
            .as_table_mut()?
            .entry("admonish")
            .or_insert(empty_table);
        item["command"] = value("mdbook-admonish");
        Some(())
    }

    fn has_file(elem: &Array, file: &str) -> bool {
        elem.iter().any(|elem| match elem.as_str() {
            None => false,
            Some(s) => s == file,
        })
    }
}
