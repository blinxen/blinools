use std::{fs, io, path::Path};

use clap::Command;
use clap_mangen::{Man, roff};

pub fn generate(command: Command, out_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(out_dir)?;
    let mut command = command.disable_help_subcommand(true);
    command.build();

    let date = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date()
        .to_string();
    render(&command, None, out_dir, &date)
}

fn page_name(command: &Command) -> &str {
    command.get_display_name().unwrap_or(command.get_name())
}

fn children(command: &Command) -> impl Iterator<Item = &Command> {
    command
        .get_subcommands()
        .filter(|child| !child.is_hide_set())
}

fn descendants(command: &Command, names: &mut Vec<String>) {
    for child in children(command) {
        names.push(format!("{}(1)", page_name(child)));
        descendants(child, names);
    }
}

fn render(command: &Command, parent: Option<&str>, out_dir: &Path, date: &str) -> io::Result<()> {
    let name = page_name(command);
    // clap_mangen 0.3 omits option values in the synopsis. Use clap's usage instead.
    let usage = command.clone().render_usage().to_string();
    let usage = usage.strip_prefix("Usage: ").unwrap_or(&usage).to_owned();
    let man = Man::new(command.clone().override_usage(usage))
        .date(date)
        .source(format!("blinools {}", env!("CARGO_PKG_VERSION")))
        .manual("blinools manual");

    let mut output = Vec::new();
    man.render_title(&mut output)?;
    man.render_name_section(&mut output)?;
    man.render_synopsis_section(&mut output)?;
    man.render_description_section(&mut output)?;
    man.render_options_section(&mut output)?;
    if children(command).next().is_some() {
        man.render_subcommands_section(&mut output)?;
    }

    if let Some(help) = command.get_after_long_help().or(command.get_after_help()) {
        let help = help.to_string();
        let mut examples = roff::Roff::default();
        examples.control("SH", ["EXAMPLES"]);
        examples.control("nf", []);
        for line in help.strip_prefix("Examples:\n").unwrap_or(&help).lines() {
            examples.text([roff::roman(line)]);
        }
        examples.control("fi", []);
        examples.to_writer(&mut output)?;
    }

    let extra: &[u8] = match name {
        "blinools" => include_bytes!("../docs/man/blinools.1.inc"),
        "blinools-sandbox" => include_bytes!("../docs/man/blinools-sandbox.1.inc"),
        _ => b"",
    };
    output.extend_from_slice(extra);
    output.extend_from_slice(include_bytes!("../docs/man/common.1.inc"));

    let mut references = Vec::new();
    if let Some(parent) = parent {
        references.push("blinools(1)".to_owned());
        if parent != "blinools" {
            references.push(format!("{parent}(1)"));
        }
    }
    descendants(command, &mut references);
    if name == "blinools" || name.starts_with("blinools-sandbox") {
        references.push("blinools.toml(5)".to_owned());
    }
    references.sort();
    references.dedup();
    let mut see_also = roff::Roff::default();
    see_also.control("SH", ["SEE ALSO"]);
    see_also.text([roff::roman(references.join(", "))]);
    see_also.to_writer(&mut output)?;

    fs::write(out_dir.join(man.get_filename()), output)?;
    for child in children(command) {
        render(child, Some(name), out_dir, date)?;
    }
    Ok(())
}
