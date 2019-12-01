use std::process::Command;
use std::collections::HashMap;
use tempfile::NamedTempFile;
use std::io::Write;
use std::io;

extern crate regex;
extern crate term;
extern crate difference;

use regex::{Regex, Captures};
use difference::{Difference, Changeset};

fn exec_mypy(file_name: &str, shadow_name: &str) -> io::Result<String>{
    let prev_mypy_output = Command::new("mypy")
        .args(&["--strict", "--shadow-file", &shadow_name, &file_name, &shadow_name])
        .output()?;

    let text = String::from_utf8(prev_mypy_output.stdout).unwrap();
    Ok(text)
}

fn exec_git_diff_and_calc_index_map(file_name: &str) -> io::Result<HashMap<usize, usize>>{
    let diff_output = Command::new("git")
    .args(&["--no-pager", "diff", "--no-ext", "-U1000000", "--", &file_name])
    .output()?;

    let diff_string = String::from_utf8(diff_output.stdout).unwrap();
    let diff_splitted = diff_string.split("\n")
        .skip_while(|&line| !line.starts_with("@@"))
        .skip(1);

    let unchanged_line_idx_prev = diff_splitted.clone()
        .filter(|&line| !line.starts_with("+"))
        .enumerate()
        .filter(|(_, line)| !line.starts_with("-"))
        .map(|(idx, _)| idx);
    let unchanged_line_idx_after = diff_splitted.clone()
        .filter(|&line| !line.starts_with("-"))
        .enumerate()
        .filter(|(_, line)| !line.starts_with("+"))
        .map(|(idx, _)| idx);

    Ok(unchanged_line_idx_prev.zip(unchanged_line_idx_after).collect())
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

#[allow(unused_must_use)]
fn main() {

    let filename_output = Command::new("git")
        .args(&["diff", "--name-only"])
        .output()
        .expect("failed to execute process");

    let filenames: String = String::from_utf8(filename_output.stdout).unwrap();
    for name in filenames.split("\n").filter(|&line| line.ends_with(".py")){
        println!("{}", &name);
     
        let prev_output = Command::new("git")
            .args(&["--no-pager", "show", &format!("{}:{}", "HEAD", &name)])
            .output()
            .expect("failed to execute process");

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&prev_output.stdout[..]).expect("failed to write");

        let tmp_name = file.path().to_str().unwrap();
        let prev_mypy_string = exec_mypy(tmp_name, name).expect("failed to exec mypy");
        let after_mypy_string = exec_mypy(name, name).expect("failed to exec mypy");
        
        let index_map = exec_git_diff_and_calc_index_map(&name).expect("failed to exec git");
        let line_idx_re = Regex::new(&format!(r"({}:)(\d+)(:)", &name)).unwrap();
        let replaced_prev_mypy_string = line_idx_re.replace_all( &prev_mypy_string,
            |caps: &Captures| {
                let num: usize = (&caps[2]).parse().unwrap();
                format!("{}{}{}",&caps[1], index_map[&num], &caps[3])
            }
        );

        print_diff(&replaced_prev_mypy_string, &after_mypy_string).expect("failed to write term");
    }

}
