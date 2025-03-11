#![allow(dead_code)]

use clap::{App, Arg, ArgAction};
use colored::Colorize;
use std::io::Result;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::{env, fs};

const WIDTH: usize = 20;
const FILENAME_RENDER_LIMIT: usize = 60;

enum ContentType {
    CODE,
    MEDIA,
    EXECUTABLE,
    NORMAL,
    TEXT,
    LICENSE,
    MAKEFILE,
}

trait Visible {
    fn is_visible(&self) -> bool;
}

impl Visible for Path {
    fn is_visible(&self) -> bool {
        //this may be a bit much. but it works well.
        if self.file_name().unwrap().to_str().unwrap().chars().nth(0) == Some('.') {
            return false;
        }
        true
    }
}

trait UnixExecutable {
    fn is_unix_executable(&self) -> Result<bool>;
}

impl UnixExecutable for Path {
    fn is_unix_executable(&self) -> Result<bool> {
        let metadata = self.metadata()?;

        Ok(metadata.permissions().mode() & 0o111 != 0)
    }
}

trait Content {
    fn content_type(&self) -> ContentType;
}

impl Content for Path {
    fn content_type(&self) -> ContentType {
        let code_extensions = vec![
            // General programming languages
            "c",
            "h",
            "cpp",
            "hpp",
            "cc",
            "cxx",
            "hh",
            "hxx", // C/C++
            "cs",
            "java",
            "class",
            "jar",
            "kt",
            "kts", // C#, Java, Kotlin
            "js",
            "jsx",
            "mjs",
            "cjs",
            "ts",
            "tsx", // JavaScript, TypeScript
            "py",
            "pyc",
            "pyd",
            "pyo",
            "rb",
            "erb", // Python, Ruby
            "php",
            "phar",
            "go",
            "rs",
            "rlib",
            "swift",
            "dart", // PHP, Go, Rust, Swift, Dart
            "scala",
            "lua",
            "r",
            "pl",
            "pm",
            "sql", // Scala, Lua, R, Perl, SQL
            // Web & Stylesheets
            "html",
            "htm",
            "xhtml",
            "xml",
            "css",
            "scss",
            "sass",
            // Config & Data
            "json",
            "yaml",
            "yml",
            "toml",
            "env",
            "ini",
            "cfg",
            // Documentation & Build
            "md",
            "rst",
            "cmake",
            "mk",
            // DevOps & Version Control
            "dockerfile",
            "dockerignore",
            "gitignore",
            "gitattributes",
        ];

        let media_extensions = vec![
            // Image extensions
            "png", "jpg", "jpeg", "gif", "bmp", "tiff", "tif", "webp", "svg", "ico", "heic", "avif",
            // Audio extensions
            "mp3", "wav", "flac", "aac", "ogg", "opus", "m4a", "wma", "aiff", "alac", "amr",
            // Video extensions
            "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpeg", "mpg", "3gp", "ogv",
        ];

        let executable_extensions = vec![
            // Windows executables
            "exe", "bat", "cmd", "msi", // Unix/Linux/macOS binary executables
            "run", "out", "bin", "app", // Java executables
            "jar", "sh", "bash", "zsh", // Scripts
            "ps1", "psm1", "psd1", // PowerShell (Windows)
        ];

        let text_extensions = vec![
            // Plain text formats
            "txt", "md", "rtf", "csv", "log", // Documentation formats (including PDF)
            "pdf", "doc", "docx", "odt", "tex", "pages",
        ];

        if code_extensions.contains(&self.extension().unwrap_or_default().to_str().unwrap()) {
            return ContentType::CODE;
        } else if media_extensions.contains(&self.extension().unwrap_or_default().to_str().unwrap())
        {
            return ContentType::MEDIA;
        } else if executable_extensions
            .contains(&self.extension().unwrap_or_default().to_str().unwrap())
            || self.is_unix_executable().unwrap()
                && !text_extensions
                    .contains(&self.extension().unwrap_or_default().to_str().unwrap())
        {
            return ContentType::EXECUTABLE;
        } else if text_extensions.contains(&self.extension().unwrap_or_default().to_str().unwrap())
        {
            return ContentType::TEXT;
        } else if self.file_name().unwrap() == "LICENSE" {
            return ContentType::LICENSE;
        } else if self.file_name().unwrap() == "Makefile" {
            return ContentType::MAKEFILE;
        } else {
            return ContentType::NORMAL;
        }
    }
}

fn fetch_gitignore(path: &Path) -> Result<Vec<String>> {
    let gitignore = path.join(".gitignore");
    if !gitignore.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(gitignore)?;
    let mut list_to_ignore: Vec<String> = Vec::new();

    for line in contents.lines() {
        let mut value = line.to_string();
        if value.starts_with("/") {
            value.remove(0);
        }
        list_to_ignore.push(value);
    }

    Ok(list_to_ignore)
}

//Non-asynchronous Linecount func, single-threaded. Only here incase I decide to reimplement it.
fn linecount(dir: Option<PathBuf>, byte_toggle: bool, ignore_toggle: bool) -> Result<(u128, u128)> {
    let (mut total_lines, mut total_bytes) = (0, 0);

    let dir_path_binding = dir.unwrap_or(env::current_dir()?);
    let dir_path = dir_path_binding.as_path();

    let directory_entries = fs::read_dir(dir_path)?;
    for entry in directory_entries {
        let entry = entry?.path();
        let path = entry.as_path();

            let metadata = fs::metadata(path)?.file_type();
            if metadata.is_file() {
                let content = String::from_utf8_lossy(&fs::read(&entry)?).into_owned();
                total_lines += content.lines().count() as u128;
                if byte_toggle {
                    total_bytes += content.as_bytes().len() as u128;
                }
                continue;
            }

            if metadata.is_dir() {
                let clone_entry = entry.clone();
                let _linecount_result = linecount(Some(entry).clone(), byte_toggle, ignore_toggle);
                let linecount = match _linecount_result {
                    Ok(success) => success,
                    Err(err) => {
                        eprintln!("{err}: skipping {:?}", Some(clone_entry));
                        continue;
                    }
                };
                total_lines += linecount.0;
                total_bytes += linecount.1;
                continue;
            };
    }
    Ok((total_lines, total_bytes))
}

fn linecount_async(dir: Option<PathBuf>, ignore_toggle: bool) -> Result<(u128, u128)> {
    let total_lines = Arc::new(Mutex::new(0));
    let total_bytes = Arc::new(Mutex::new(0));
    let dir_path_binding = dir.unwrap_or(env::current_dir()?);
    let dir_path = dir_path_binding.as_path();
    //let ignore_vec = fetch_gitignore(&dir_path)?;
    let mut handles = Vec::new();

    let entries = fs::read_dir(dir_path)
        .expect("Failed to read directory")
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();

    for entry in entries {
        let path = entry.as_path();
        let filetype = fs::metadata(path)?.file_type();

        if filetype.is_file() {
            let content = String::from_utf8_lossy(&fs::read(&path)?).into_owned();
            let file_linecount = content.lines().count() as u128;
            let file_bytes = content.as_bytes().len() as u128;

            *total_lines.lock().unwrap() += file_linecount;
            *total_bytes.lock().unwrap() += file_bytes;
        } else if filetype.is_dir() {
            let handle = {
                let total_lines = Arc::clone(&total_lines);
                let total_bytes = Arc::clone(&total_bytes);
                let path = PathBuf::from(path);

                thread::spawn(move || {
                    let recursive_lc = linecount_async(Some(path), ignore_toggle);

                    if let Ok((lines, bytes)) = recursive_lc {
                        *total_lines.lock().unwrap() += lines;
                        *total_bytes.lock().unwrap() += bytes;
                    }
                })
            };
            handles.push(handle);
        }
    }
    for handle in handles {
        handle.join().unwrap();
    }

    Ok(get_totals(total_lines, total_bytes))
}

fn linecount_display(
    dir: Option<PathBuf>,
    ignore_toggle: bool,
    mut indent_amount: Option<usize>,
) -> Result<(u128, u128)> {
    let (mut total_lines, mut total_bytes) = (0, 0);
    let dir_path_binding = dir.unwrap_or(env::current_dir()?);
    let dir_path = dir_path_binding.as_path();
    let mut file_indent_from_zero_size = indent_amount.unwrap_or_default();
    //let ignore_vec = fetch_gitignore(&dir_path)?;

    if indent_amount.is_none() {
        indent_amount = Some(0);
    } else if indent_amount.unwrap() > 0 {
        file_indent_from_zero_size += 1;
    }

    let (dir_indent, file_indent_from_dir, file_ident_from_zero) = (
        "─".repeat(indent_amount.unwrap_or_default()),
        "─".repeat(2),
        " ".repeat(file_indent_from_zero_size),
    );
    let dir_path_str = dir_path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap_or_default()
        .blue()
        .bold();

    match indent_amount {
        Some(0) => println!("{dir_indent}{dir_path_str}/"),
        _ => println!("├{dir_indent}{dir_path_str}/"),
    }

    let entries = fs::read_dir(dir_path)
        .expect("Failed to read directory")
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    let (mut files, mut dirs) = (Vec::new(), Vec::new());

    for entry in entries {
        //if ignore_toggle {
        //    if ignore_vec.contains(&entry.file_name().unwrap().to_string_lossy().to_string()) {
        //        continue;
        //    }
        //}

        if entry.is_file() {
            files.push(entry);
        } else {
            dirs.push(entry);
        }
    }
    files.sort();
    dirs.sort();
    let sorted_entries = files.iter().chain(dirs.iter());

    for (idx, entry) in sorted_entries.enumerate() {
        let mut connector = "├";
        let path = entry.as_path();
        let filetype = fs::metadata(path)?.file_type();

        if filetype.is_file() {
            let content = String::from_utf8_lossy(&fs::read(&path)?).into_owned();
            let file_linecount = content.lines().count() as u128;
            let file_bytes = content.as_bytes().len() as u128;

            total_lines += file_linecount;
            total_bytes += file_bytes;

            let filename = entry
                .file_name()
                .unwrap()
                .to_str()
                .unwrap_or("?")
                .to_string();

            let filename = if filename.len() > FILENAME_RENDER_LIMIT {
                format!("{}...", &filename[..FILENAME_RENDER_LIMIT])
            } else {
                filename
            };
            if idx == files.len() - 1 {
                connector = "└";
            }

            let formatted_indent = match indent_amount {
                Some(0) => format!("{file_ident_from_zero}{connector}{file_indent_from_dir}"),
                _ => format!("│{file_ident_from_zero}{connector}{file_indent_from_dir}"),
            };

            let formatted_output = format!(
                "{:width$} ({}L, {}B)",
                {
                    match path.content_type() {
                        ContentType::MEDIA => filename.bright_magenta().to_string(),
                        ContentType::CODE => filename.cyan().to_string(),
                        ContentType::EXECUTABLE => filename.green().to_string(),
                        ContentType::TEXT => filename.truecolor(217, 50, 122).to_string(),
                        ContentType::LICENSE => filename.truecolor(0, 0, 255).to_string(),
                        ContentType::MAKEFILE => filename.red().to_string(),
                        _ => filename.to_string(),
                    }
                },
                file_linecount,
                file_bytes,
                width = WIDTH
            );
            println!("{formatted_indent}{formatted_output}");
        } else if filetype.is_dir() {
            if let Ok((lines, bytes)) = linecount_display(
                Some(PathBuf::from(&path)),
                ignore_toggle,
                Some(indent_amount.unwrap_or_default() + 2),
            ) {
                total_lines += lines;
                total_bytes += bytes;
            }
        };
    }
    Ok((total_lines, total_bytes))
}

//EXPERIMENTAL: runs linecount_display via paralellization. has significant increase in speed.
//   -BUGS: since the function operates in parrell, printing the treemap is unreliable since order is not guaranteed.
//          because of this the output looks scattered and disorganized.
//
//
fn linecount_visual_async(
    dir: Option<PathBuf>,
    ignore_toggle: bool,
    mut indent_amount: Option<usize>,
) -> Result<(u128, u128)> {
    let total_lines = Arc::new(Mutex::new(0));
    let total_bytes = Arc::new(Mutex::new(0));
    let dir_path_binding = dir.unwrap_or(env::current_dir()?);
    let dir_path = dir_path_binding.as_path();
    let mut file_indent_from_zero_size = indent_amount.unwrap_or_default();
    //let ignore_vec = fetch_gitignore(&dir_path)?;
    let mut handles = Vec::new();

    if indent_amount.is_none() {
        indent_amount = Some(0);
    } else if indent_amount.unwrap() > 0 {
        file_indent_from_zero_size += 1;
    }

    let (dir_indent, file_indent_from_dir, file_ident_from_zero) = (
        "─".repeat(indent_amount.unwrap_or_default()),
        "─".repeat(2),
        " ".repeat(file_indent_from_zero_size),
    );
    let dir_path_str = dir_path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap_or_default()
        .blue()
        .bold();

    match indent_amount {
        Some(0) => println!("{dir_indent}{dir_path_str}/"),
        _ => println!("├{dir_indent}{dir_path_str}/"),
    }

    let entries = fs::read_dir(dir_path)
        .expect("Failed to read directory")
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    let (mut files, mut dirs) = (Vec::new(), Vec::new());

    for entry in entries {
        //if ignore_toggle {
        //    if ignore_vec.contains(&entry.file_name().unwrap().to_string_lossy().to_string()) {
        //        continue;
        //    }
        //}

        if entry.is_file() {
            files.push(entry);
        } else {
            dirs.push(entry);
        }
    }
    files.sort();
    dirs.sort();
    let sorted_entries = files.iter().chain(dirs.iter());

    for (idx, entry) in sorted_entries.enumerate() {
        let mut connector = "├";
        let path = entry.as_path();
        let filetype = fs::metadata(path)?.file_type();

        if filetype.is_file() {
            let content = String::from_utf8_lossy(&fs::read(&path)?).into_owned();
            let file_linecount = content.lines().count() as u128;
            let file_bytes = content.as_bytes().len() as u128;

            *total_lines.lock().unwrap() += file_linecount;
            *total_bytes.lock().unwrap() += file_bytes;

                let filename = entry
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap_or("?")
                    .to_string();

                let filename = if filename.len() > FILENAME_RENDER_LIMIT {
                    format!("{}...", &filename[..FILENAME_RENDER_LIMIT])
                } else {
                    filename
                };
                if idx == files.len() - 1 {
                    connector = "└";
                }

                let formatted_indent = match indent_amount {
                    Some(0) => format!("{file_ident_from_zero}{connector}{file_indent_from_dir}"),
                    _ => format!("│{file_ident_from_zero}{connector}{file_indent_from_dir}"),
                };

                let formatted_output = format!(
                    "{:width$} ({}L, {}B)",
                    {
                        match path.content_type() {
                            ContentType::MEDIA => filename.bright_magenta().to_string(),
                            ContentType::CODE => filename.cyan().to_string(),
                            ContentType::EXECUTABLE => filename.green().to_string(),
                            ContentType::TEXT => filename.truecolor(217, 50, 122).to_string(),
                            ContentType::LICENSE => filename.truecolor(0, 0, 255).to_string(),
                            ContentType::MAKEFILE => filename.red().to_string(),
                            _ => filename.to_string(),
                        }
                    },
                    file_linecount,
                    file_bytes,
                    width = WIDTH
                );
                println!("{formatted_indent}{formatted_output}");
        } else if filetype.is_dir() {
            let handle = {
                let total_lines = Arc::clone(&total_lines);
                let total_bytes = Arc::clone(&total_bytes);
                let path = PathBuf::from(path);

                thread::spawn(move || {
                    let recursive_lc = linecount_visual_async(
                        Some(path),
                        ignore_toggle,
                        Some(indent_amount.unwrap() + 2),
                    );

                    if let Ok((lines, bytes)) = recursive_lc {
                        *total_lines.lock().unwrap() += lines;
                        *total_bytes.lock().unwrap() += bytes;
                    }
                })
            };
            handles.push(handle);
        }
    }
    for handle in handles {
        handle.join().unwrap();
    }

    Ok(get_totals(total_lines, total_bytes))
}

fn get_totals(total_lines: Arc<Mutex<u128>>, total_bytes: Arc<Mutex<u128>>) -> (u128, u128) {
    let lines = total_lines.lock().unwrap();
    let bytes = total_bytes.lock().unwrap();
    (*lines, *bytes)
}

fn format_byte_count(byte_count: u128) -> String {
    if byte_count / 1_000_000_000 > 1 {
        return format!("{} GB", byte_count as f64 / 1_000_000_000.);
    } else if byte_count / 1_000_000 > 1 {
        return format!("{} MB", byte_count as f64 / 1_000_000.);
    } else if byte_count / 1_000 > 1 {
        return format!("{} KB", byte_count as f64 / 1_000.);
    } else {
        return format!("{} B", byte_count);
    }
}

fn format_and_print_results(lines: u128, bytes: u128, time: Duration) {
    let f_bytes = format_byte_count(bytes);
    println!("╭───────────────────────────────────────────────────╮");
    println!(
        "│{:<51}│\n│{:<51}│\n│{:<51}│",
        format!("Lines       :{lines}"),
        format!("Bytes       :{f_bytes}"),
        format!("Time Taken  :{:.5} Seconds", time.as_secs_f64())
    );
    println!("╰───────────────────────────────────────────────────╯")
}

fn main() -> std::io::Result<()> {
    let calls = App::new("lc")
        .version("1.2")
        .author("Ethan Water")
        .about("Line Counting Program")
        .arg(
            Arg::new("path")
                .short('p')
                .long("path")
                .action(ArgAction::Set)
                .value_name("PATH")
                .help("Provides a path to lc"),
        )
        .arg(Arg::new("display").short('d').long("display"))
        .arg(Arg::new("test-async").long("test-async"))
        .get_matches();

    let path = calls.get_one::<String>("path").map(PathBuf::from);

    if calls.contains_id("test-async") {
        let start_time = Instant::now();
        let (lines, bytes) = linecount_visual_async(path, true, None)?;
        let end_time = Instant::now();
        format_and_print_results(lines, bytes, end_time-start_time);
    } else if calls.contains_id("display") {
        let start_time = Instant::now();
        let (lines, bytes) = linecount_display(path, true, None)?;
        let end_time = Instant::now();
        format_and_print_results(lines, bytes, end_time-start_time);
    } else {
        let start_time = Instant::now();
        let (lines, bytes) = linecount_async(path, false)?;
        let end_time = Instant::now();
        format_and_print_results(lines, bytes, end_time-start_time);
    }
    Ok(())
}
