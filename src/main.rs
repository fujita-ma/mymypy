use std::process::Command;
use std::collections::BTreeMap;
use tempfile::NamedTempFile;
use std::io::Write;
use std::io;
use std::env;

extern crate regex;
extern crate term;
extern crate difference;

use regex::{Regex, Captures};
use difference::{Difference, Changeset};

/// execute mypy and return stdout as String
fn exec_mypy(file_name: &str, shadow_name: &str) -> io::Result<String>{
    let mypy_output = Command::new("mypy")
        .args(&["--strict", "--shadow-file", &shadow_name, &file_name, &shadow_name])
        .output()?;

    let text = String::from_utf8(mypy_output.stdout).unwrap();
    Ok(text)
}

/// write bytes from git show ref_name:file_name
fn write_file_from_git(handle: &mut NamedTempFile, ref_name: &str, file_name: &str) -> (){
    let output = Command::new("git")
        .args(&["--no-pager", "show", &format!("{}:{}", ref_name, &file_name)])
        .output()
        .expect("failed to execute process");
    handle.write_all(&output.stdout[..]).expect("failed to write");
}

/// get int -> int map, which projects line index of original file to edited file
fn calc_index_map(diff_output: Vec<u8>) -> (BTreeMap<usize, usize>, BTreeMap<usize, usize>){
    let diff_string = String::from_utf8(diff_output).unwrap();
    let diff_splitted = diff_string.split("\n")
        .skip_while(|&line| !line.starts_with("@@"))
        .skip(1);

    let unchanged_line_idx_orig = diff_splitted.clone()
        .filter(|&line| !line.starts_with("+"))
        .enumerate()
        .filter(|(_, line)| !line.starts_with("-"))
        .map(|(idx, _)| idx + 1);
    let unchanged_line_idx_edit = diff_splitted
        .filter(|&line| !line.starts_with("-"))
        .enumerate()
        .filter(|(_, line)| !line.starts_with("+"))
        .map(|(idx, _)| idx + 1);

    (
        unchanged_line_idx_orig.clone().zip(unchanged_line_idx_edit.clone()).collect(),
        unchanged_line_idx_edit.zip(unchanged_line_idx_orig).collect()
    )
}

/// display diff with color in terminal
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

    // extract edited filenames
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
        
        // get mypy result of original file
        let mut orig_tmp_file = NamedTempFile::new().unwrap();
        write_file_from_git(&mut orig_tmp_file, orig_ref, name);
        let orig_name = orig_tmp_file.path().to_str().unwrap();
        let orig_mypy_string = exec_mypy(orig_name, name).expect("failed to exec mypy");
        let orig_mypy_lines = orig_mypy_string.split("\n");

        // get mypy result of edited file
        let mut edit_tmp_file: NamedTempFile;
        let edit_name = match edit_ref{
            Some(ref_name) => {
                edit_tmp_file = NamedTempFile::new().unwrap();
                write_file_from_git(&mut edit_tmp_file, ref_name, name);
                edit_tmp_file.path().to_str().unwrap()
            },
            None => name
        };
        let edit_mypy_string = exec_mypy(edit_name, name).expect("failed to exec mypy");
        let edit_mypy_lines = edit_mypy_string.split("\n");

        // map line indices that are invariant between the two files
        let diff_output = Command::new("git")
        .args(match edit_ref{
            Some(ref_name) => vec!["--no-pager", "diff", "--no-ext", "-U1000000", orig_ref, ref_name, "--", &name],
            None => vec!["--no-pager", "diff", "--no-ext", "-U1000000", orig_ref, "--", &name]
        })
        .output().expect("failed to exec git diff");
        let (index_map_from_orig, index_map_from_edit) = calc_index_map(diff_output.stdout);

        let line_idx_re = Regex::new(&format!(r"^({}:)(\d+)(:)", &name.replace("/", r"[\\/]"))).unwrap();
        let replaced_orig_mypy_string = orig_mypy_lines
            .filter(|&line| line_idx_re.is_match(line))
            .map(|line| line_idx_re.replace(line, |caps: &Captures| {
                let num: usize = (&caps[2]).parse().unwrap();
                let edit_num_str = match index_map_from_orig.get(&num){
                    Some(num) => num.to_string(),
                    None => String::from("_")
                };
                format!("{}{}:{}{}",&caps[1], num, edit_num_str, &caps[3])
            }))
            .collect::<Vec<String>>()
            .join("\n");

            
        let replaced_edit_mypy_string = edit_mypy_lines
            .filter(|&line| line_idx_re.is_match(line))
            .map(|line| line_idx_re.replace(line, |caps: &Captures| {
                let num: usize = (&caps[2]).parse().unwrap();
                let orig_num_str = match index_map_from_edit.get(&num){
                    Some(num) => num.to_string(),
                    None => String::from("_")
                };
                format!("{}{}:{}{}",&caps[1], orig_num_str, num, &caps[3])
            }))
            .collect::<Vec<String>>()
            .join("\n");

        // dispay two mypy result
        print_diff(&replaced_orig_mypy_string, &replaced_edit_mypy_string).expect("failed to write term");
    }

}
