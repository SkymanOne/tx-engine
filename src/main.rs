use tx_engine::{path_reader, print_accounts, process_txs, types::Accounts};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let mut reader = path_reader(path).unwrap();
    let mut accounts = Accounts::default();
    process_txs(&mut reader, &mut accounts);

    // Normalize and print accounts to stdout
    print_accounts(&mut accounts).expect("Failed to write accounts to stdout");
}
