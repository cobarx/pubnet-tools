fn main() {
    println!("# pubnetdiag exit codes\n");
    println!("| Code | Name | Meaning |");
    println!("|------|------|---------|");
    for (code, name, desc) in pubnetdiag::exit_codes::TABLE {
        println!("| {code} | `{name}` | {desc} |");
    }
}
