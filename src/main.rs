use std::process::Command;
use std::collections::HashMap;
use tempfile::NamedTempFile;
use std::io::Write;
use std::io;
use std::env;

extern crate regex;
extern crate term;
extern crate difference;

use regex::{Regex, Captures};
use difference::{Difference, Changeset};

fn exec_mypy(file_name: &str, shadow_name: &str) -> io::Result<String>{
    let mypy_output = Command::new("mypy")
        .args(&["--strict", "--shadow-file", &shadow_name, &file_name, &shadow_name])
        .output()?;

    let text = String::from_utf8(mypy_output.stdout).unwrap();
    Ok(text)
}

fn create_tmp_file_from_git<'a>(handle: &'a mut NamedTempFile, ref_name: &str, file_name: &str) -> &'a str{
    let output = Command::new("git")
        .args(&["--no-pager", "show", &format!("{}:{}", ref_name, &file_name)])
        .output()
        .expect("failed to execute process");
    handle.write_all(&output.stdout[..]).expect("failed to write");
    handle.path().to_str().unwrap()
}

fn calc_index_map(diff_output: Vec<u8>) -> HashMap<usize, usize>{
    let diff_string = String::from_utf8(diff_output).unwrap();
    let diff_splitted = diff_string.split("\n")
        .skip_while(|&line| !line.starts_with("@@"))
        .skip(1);

    let unchanged_line_idx_orig = diff_splitted.clone()
        .filter(|&line| !line.starts_with("+"))
        .enumerate()
        .filter(|(_, line)| !line.starts_with("-"))
        .map(|(idx, _)| idx);
    let unchanged_line_idx_edit = diff_splitted.clone()
        .filter(|&line| !line.starts_with("-"))
        .enumerate()
        .filter(|(_, line)| !line.starts_with("+"))
        .map(|(idx, _)| idx);

    unchanged_line_idx_orig.zip(unchanged_line_idx_edit).collect()
}

fn print_diff(orig: &str, edit: &str) -> io::Result<()>{
    let Changeset { diffs, .. } = Changeset::new(&orig, &edit, "\n");

    let mut t = term::stdout().unwrap();

    for i in 0..diffs.len() {
        match diffs[i] {
            Difference::Same(ref x) => {
                t.reset()?;
                writeln!(t, "{}", x)?;
            }
            Difference::Add(ref x) => {
                t.fg(term::color::GREEN)?;
                writeln!(t, "+{}", x)?;
            }
            Difference::Rem(ref x) => {
                t.fg(term::color::RED)?;
                writeln!(t, "-{}", x)?;
            }
        }
    }
    t.reset()?;
    t.flush()?;
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let orig_ref = match args.get(1){
        Some(ref_name) => ref_name,
        None => "HEAD"
    };
    let edit_ref = args.get(2);

    let filename_output = Command::new("git")
        .args(match edit_ref{
            Some(ref_name) => vec!["diff", "--name-only", orig_ref, ref_name],
            None => vec!["diff", "--name-only", orig_ref]
        })
        .output()
        .expect("failed to execute process");
    let filenames: String = String::from_utf8(filename_output.stdout).unwrap();

    for name in filenames.split("\n").filter(|&line| line.ends_with(".py")){
        println!("{}", &name);
        
        let mut orig_tmp_file = NamedTempFile::new().unwrap();
        let orig_name = create_tmp_file_from_git(&mut orig_tmp_file, orig_ref, name);
        let orig_mypy_string = exec_mypy(orig_name, name).expect("failed to exec mypy");

        let mut edit_tmp_file: NamedTempFile;
        let edit_name = match edit_ref{
            Some(ref_name) => {
                edit_tmp_file = NamedTempFile::new().unwrap();
                create_tmp_file_from_git(&mut edit_tmp_file, ref_name, name)
            },
            None => name
        };
        let edit_mypy_string = exec_mypy(edit_name, name).expect("failed to exec mypy");

        let diff_output = Command::new("git")
        .args(match edit_ref{
            Some(ref_name) => vec!["--no-pager", "diff", "--no-ext", "-U1000000", orig_ref, ref_name, "--", &name],
            None => vec!["--no-pager", "diff", "--no-ext", "-U1000000", orig_ref, "--", &name]
        })
        .output().expect("failed to exec git diff");
        let index_map = calc_index_map(diff_output.stdout);

        let line_idx_re = Regex::new(&format!(r"({}:)(\d+)(:)", &name)).unwrap();
        let replaced_orig_mypy_string = line_idx_re.replace_all( &orig_mypy_string,
            |caps: &Captures| {
                let num: usize = (&caps[2]).parse().unwrap();
                format!("{}{}{}",&caps[1], index_map[&num], &caps[3])
            }
        );

        print_diff(&replaced_orig_mypy_string, &edit_mypy_string).expect("failed to write term");
    }

}
