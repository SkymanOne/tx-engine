pub mod types;

use anyhow::Context;
use csv::{Reader, ReaderBuilder, WriterBuilder};
use types::TxType;

use crate::types::{AccountRow, Accounts, CsvRow, DisputeState};

#[cfg(test)]
mod tests;

/// Print accounts as CSV to STDOUT
pub fn print_accounts(accounts: &mut Accounts) -> anyhow::Result<()> {
    let mut writer = WriterBuilder::new()
        .has_headers(true)
        .from_writer(std::io::stdout());

    // Write all rows with headers
    for entry in accounts {
        let id = entry.0;
        let account = entry.1;

        // normalise amounts to 4dp
        account.normalize();
        let account_row = AccountRow {
            client: *id,
            available: account.available,
            held: account.held,
            total: account.total,
            locked: account.locked,
        };

        writer.serialize(account_row)?;
    }

    writer.flush()?;
    Ok(())
}

/// Process all transactions lazily from an iterator
pub fn process_txs<R: std::io::Read>(reader: &mut Reader<R>, accounts: &mut Accounts) {
    let txs = record_iter(reader);
    for tx_res in txs {
        // ignore malformed entries
        let Ok(tx) = tx_res else {
            continue;
        };

        process_tx(tx, accounts);
    }
}

/// Process individual transaction
pub fn process_tx(tx: TxType, accounts: &mut Accounts) {
    match tx {
        TxType::Deposit(balance_change) => {
            let account = accounts.entry(balance_change.client).or_default();
            // if account is locked, ignore
            if account.locked {
                return;
            }

            account.available = account.available.saturating_add(balance_change.amount);
            account.total = account.total.saturating_add(balance_change.amount);

            // Assume that all tx ids are unique and overwrite will not happen
            account
                .deposit_txs
                .insert(balance_change.tx, balance_change.into());
        }
        TxType::Withdrawal(balance_change) => {
            let account = accounts.entry(balance_change.client).or_default();
            // if account is locked, ignore
            if account.locked {
                return;
            }

            // if available balance is lower than withdrawal amount, ignore tx
            if account.available < balance_change.amount {
                return;
            }

            account.available = account.available.saturating_sub(balance_change.amount);
            account.total = account.total.saturating_sub(balance_change.amount);
        }
        TxType::Dispute(dispute) => {
            // if account doesn't exist, there is nothing to dispute
            let Some(account) = accounts.get_mut(&dispute.client) else {
                return;
            };

            // if deposit tx doesn't exit, there is nothing to dispute
            let Some(deposit) = account.deposit_txs.get_mut(&dispute.tx) else {
                return;
            };

            // if deposit tx is already disputed, we ignore it
            if deposit.dispute != DisputeState::None {
                return;
            }

            // update balances and tx status
            account.held = account.held.saturating_add(deposit.amount);
            account.available = account.available.saturating_sub(deposit.amount);
            deposit.dispute = DisputeState::Disputed;
        }
        TxType::Resolve(dispute) => {
            // if account doesn't exist, there is nothing to resolve
            let Some(account) = accounts.get_mut(&dispute.client) else {
                return;
            };

            // if deposit tx doesn't exit, there is nothing to resolve
            let Some(deposit) = account.deposit_txs.get_mut(&dispute.tx) else {
                return;
            };

            // if deposit tx isn't disputed, we ignore it
            if deposit.dispute != DisputeState::Disputed {
                return;
            }

            // update balances and tx status
            account.held = account.held.saturating_sub(deposit.amount);
            account.available = account.available.saturating_add(deposit.amount);
            // transaction is no longer disputed
            deposit.dispute = DisputeState::None;
        }
        TxType::Chargeback(dispute) => {
            // if account doesn't exist, there is nothing to chargeback
            let Some(account) = accounts.get_mut(&dispute.client) else {
                return;
            };

            // if deposit tx doesn't exit, there is nothing to chargeback
            let Some(deposit) = account.deposit_txs.get_mut(&dispute.tx) else {
                return;
            };

            // if deposit tx isn't disputed, we ignore it
            if deposit.dispute != DisputeState::Disputed {
                return;
            }

            // update balances and tx status
            account.held = account.held.saturating_sub(deposit.amount);
            account.total = account.total.saturating_sub(deposit.amount);
            // transaction is no longer disputed
            deposit.dispute = DisputeState::Chargeback;
            // Lock the account
            account.locked = true;
        }
    }
}

/// Return an iterator of parsed transaction records
pub fn record_iter<R: std::io::Read>(
    reader: &mut Reader<R>,
) -> impl Iterator<Item = anyhow::Result<TxType>> {
    reader.deserialize::<CsvRow>().map(|res| {
        res.context("Failed to deserialize the record")
            .and_then(TxType::try_from)
    })
}

/// Construct file reader for CSV input.
pub fn path_reader(path: &str) -> anyhow::Result<Reader<std::fs::File>> {
    ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .context(format!("Failed to open {path}"))
}
