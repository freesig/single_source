use clap::ArgMatches;
use clap::{App, SubCommand};
use regex::Regex;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;

const INCLUDE: &'static str = "INCLUDE";
const SKIP: &'static str = "SKIP";
const HIDE: &'static str = "HIDE";
const EXTERNAL: &'static str = "EXTERNAL";
const CHECK: &'static str = "CHECK";
const EXTRA: &'static str = "EXTRA";
const MODE: &'static str = "MODE";
const CHANGE: &'static str = "CHANGE";

#[derive(Debug)]
struct Mode {
    m: String,
    state: ModeState,
}
#[derive(Clone, Debug)]
enum ModeState {
    Active,
    Disabled,
    Off,
}

fn main() -> std::io::Result<()> {
    let matches = App::new("single source")
        .version("0.1.0")
        .author("Tom Gowan <tom.gowan@holo.host>")
        .about("Turns annotated md into working code")
        .subcommand(SubCommand::with_name("code")
                    .about("Generates code file.")
                    .args_from_usage(
                        "<INPUT>              'Sets the input md file to use'
                        <OUTPUT>              'Sets the file to render output to'
                        <LANG>              'Sets the language so only these tags will be generated ie. rust will get all ```rust tags'
                        [MODE]              'Sets the mode for this run. Is useful for generating different files from the same languag'
                        -s, --show              'shows the output of diffs'"))
        .subcommand(SubCommand::with_name("md")
                    .about("Generates code file.")
                    .args_from_usage(
                        "<INPUT>              'Sets the input md file to use'
                        <OUTPUT>              'Sets the file to render output to'"))
        .get_matches();
    if let Some(matches) = matches.subcommand_matches("code") {
        generate_code(matches)?;
    }
    if let Some(matches) = matches.subcommand_matches("md") {
        generate_md(matches)?;
    }
    Ok(())
}

fn generate_code(matches: &ArgMatches) -> std::io::Result<()> {
    let input_file = matches.value_of("INPUT").unwrap();
    let output_file = matches.value_of("OUTPUT").unwrap();
    let lang = matches.value_of("LANG").unwrap();
    let mode = matches.value_of("MODE");
    let show_diff = matches.is_present("show");

    let input_path = Path::new(&input_file).parent().expect(&format!(
        "Cannot find the markdown source at: {}",
        input_file
    ));
    let mut input_file = File::open(input_file).expect(&format!(
        "Cannot find the markdown source at: {}",
        input_file
    ));
    let mut buffer = Vec::new();
    input_file.read_to_end(&mut buffer)?;

    let input_buffer = String::from_utf8(buffer).expect("Failed to parse buffer as U8");
    let just_code = remove_non_code(&input_buffer, lang, Some(input_path), None, mode, show_diff);
    let mut out = File::create(output_file)?;
    write!(&mut out, "{}", just_code)?;
    Ok(())
}

fn generate_md(matches: &ArgMatches) -> std::io::Result<()> {
    let input_file = matches.value_of("INPUT").unwrap();
    let output_file = matches.value_of("OUTPUT").unwrap();

    let input_path = Path::new(&input_file).parent().expect(&format!(
        "Cannot find the markdown source at: {}",
        input_file
    ));

    let mut input_file = File::open(input_file).expect(&format!(
        "Cannot find the markdown source at: {}",
        input_file
    ));
    let mut buffer = Vec::new();
    input_file.read_to_end(&mut buffer)?;

    let input_buffer = String::from_utf8(buffer).expect("Failed to parse buffer as U8");
    let just_md = remove_code(&input_buffer, Some(input_path));
    let mut out = File::create(output_file)?;
    write!(&mut out, "{}", just_md)?;
    Ok(())
}

fn remove_non_code(
    buffer: &String,
    lang: &str,
    input_path: Option<&Path>,
    until: Option<usize>,
    mode: Option<&str>,
    show_diff: bool,
) -> String {
    let re_start = Regex::new(&format!("```{}.*", lang)).expect("Failed to create regex");
    let re_end = Regex::new(r"```$").expect("Failed to create regex");
    let re_tag = Regex::new(r"\\#S:([\w,=/\.]+)").expect("Failed to create regex");
    let mut keep = false;
    let mut include = false;
    let mut extra = false;
    let mut current_mode = Mode::new(mode);
    let mut output = String::with_capacity(buffer.len());
    for (i, line) in buffer.lines().enumerate() {
        if let Some(until) = until {
            if i == until {
                break;
            }
        }
        if re_start.is_match(line) {
            keep = true;
            if let ModeState::Disabled = current_mode.state {
                keep = false;
            }
            continue;
        }
        if re_end.is_match(line) {
            keep = false;
            extra = false;
            continue;
        }
        if re_tag.is_match(line) {
            for cap in re_tag.captures_iter(line) {
                let tags: Vec<&str> = cap[1].split(',').collect();
                for tag in tags.iter() {
                    match *tag {
                        INCLUDE => include = true,
                        SKIP => include = false,
                        EXTRA => extra = true,
                        CHANGE => {
                            output = change(output, buffer, i, show_diff);
                        }
                        _ => (),
                    }
                    if tag.contains(MODE) {
                        let mut t = tag.split('=');
                        t.next();
                        match (current_mode.state.clone(), t.next()) {
                            (ModeState::Active, Some(m)) if current_mode.m.as_str() != m => {
                                current_mode.state = ModeState::Disabled
                            }
                            (ModeState::Disabled, Some(m)) if current_mode.m.as_str() == m => {
                                current_mode.state = ModeState::Active
                            }
                            _ => (),
                        }
                    }
                    if tag.contains(EXTERNAL) {
                        let mut t = tag.split('=');
                        t.next();
                        if let Some(e_lang) = t.next() {
                            if e_lang != lang {
                                continue;
                            }
                        }
                        match (t.next(), input_path) {
                            (Some(external_file), Some(input_path)) => {
                                let path_to_external = input_path
                                    .to_str()
                                    .expect("Failed to pass input path")
                                    .to_owned();
                                let path_to_external =
                                    format!("{}/{}", path_to_external, external_file);
                                if let ModeState::Disabled = current_mode.state {
                                    match t.next() {
                                        Some(m) if m == current_mode.m.as_str() => (),
                                        _ => continue,
                                    }
                                }
                                output.push_str(&add_external(&path_to_external));
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
        if keep && include && !extra {
            output.push_str(&format!("{}\n", line));
        }
    }
    output
}

fn remove_code(buffer: &String, input_path: Option<&Path>) -> String {
    let re_code = Regex::new(r"```").expect("Failed to create regex");
    let re_tag = Regex::new(r"\\#S:([\w,=/\.]+)").expect("Failed to create regex");
    let mut code = false;
    let mut show = true;
    let mut extra = false;
    let mut extra_content = String::new();
    let mut output = String::with_capacity(buffer.len());
    for (i, line) in buffer.lines().enumerate() {
        if re_code.is_match(line) {
            code = !code;
            if !code {
                let s = show;
                show = true;
                if !s {
                    continue;
                }
            }
            if extra {
                continue;
            }
        }
        if re_tag.is_match(line) {
            for cap in re_tag.captures_iter(line) {
                let tags: Vec<&str> = cap[1].split(',').collect();
                for tag in tags.iter() {
                    match *tag {
                        HIDE => show = false,
                        EXTRA => extra = true,
                        _ => (),
                    }
                    if tag.contains(CHECK) {
                        let mut t = tag.split('=');
                        t.next();
                        if let Some(lang) = t.next() {
                            output.push_str("??? question \"Check your code\"\n");
                            output.push_str(&format!("    ```{}\n", lang));
                            let content = remove_non_code(
                                &buffer,
                                lang,
                                input_path,
                                Some(i),
                                t.next(),
                                false,
                            );
                            let content: String =
                                content.lines().map(|l| format!("    {}\n", l)).collect();
                            output.push_str(&content);
                            output.push_str(&extra_content);
                            output.push_str("    ```\n");
                            extra_content.clear();
                            extra = false;
                        }
                    }
                }
            }
            continue;
        }
        if !code || code && show && !extra {
            output.push_str(&format!("{}\n", line));
        }
        if extra {
            extra_content.push_str(&format!("{}\n", line));
        }
    }
    output
}

fn add_external(file: &str) -> String {
    let mut file = File::open(file).expect("Missing external file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .expect("Failed to read external file");

    String::from_utf8(buffer).expect("Failed to parse buffer as U8")
}

fn change(input: String, buffer: &String, from: usize, show_diff: bool) -> String {
    // Get diff
    let re_diff = Regex::new(&format!("```{}.*", "diff")).expect("Failed to create regex");
    let re_end = Regex::new(r"```$").expect("Failed to create regex");
    let re_add = Regex::new(r"^\+.*").expect("Failed to create regex");
    let re_sub = Regex::new(r"^\-.*").expect("Failed to create regex");
    let mut diff = false;
    let mut old = vec![];
    let mut new = vec![];
    for line in buffer.lines().skip(from) {
        if re_end.is_match(line) {
            break;
        }
        if diff {
            if re_add.is_match(line) {
                let c: String = line.chars().skip(1).collect();
                new.push(c);
            } else if re_sub.is_match(line) {
                let c: String = line.chars().skip(1).collect();
                old.push(c.trim().to_owned());
            } else {
                new.push(line.to_owned());
                old.push(line.trim().to_owned());
            }
        }
        if re_diff.is_match(line) {
            diff = true;
        }
    }

    let match_line = old.join("");
    let input_buffer: Vec<_> = input.lines().map(|l| l.trim()).collect();
    let mut output = String::with_capacity(input.len());

    let mut start_line = None;
    let step = old.len();
    if show_diff {
        println!("Match Line: {:#?}", match_line)
    }
    for i in 0..input_buffer.len() {
        if i + step > input_buffer.len() {
            break;
        }
        let squash = input_buffer[i..(i+step)].join("");
        if show_diff {
            println!("Squash: {:#?}", squash)
        }
        if squash == match_line {
            start_line = Some(i);
            if show_diff {
                println!("--- MATCH ---");
            }
        }
    }
    if let Some(start_line) = start_line {
        for s in input
            .lines()
            .take(start_line)
            .chain(new.iter().map(|s| s.as_str()))
            .chain(input.lines().skip(start_line + step))
        {
            output.push_str(&format!("{}\n", s));
        }
    } else {
        output = input;
    }

    output
}

impl Mode {
    fn new(mode: Option<&str>) -> Self {
        match mode {
            Some(m) => Mode {
                m: m.to_string(),
                state: ModeState::Disabled,
            },
            None => Mode {
                m: Default::default(),
                state: ModeState::Off,
            },
        }
    }
}
