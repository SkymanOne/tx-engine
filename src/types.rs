use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A map of user ids => account data
pub type Accounts = HashMap<u16, ClientAccount>;

/// State of a dispute
#[derive(Debug, Clone, PartialEq)]
pub enum DisputeState {
    /// Transaction is not disputed
    None,
    /// Transactions is disputed
    Disputed,
    /// Chargeback occurred after dispute
    Chargeback,
}

/// Deposit transaction
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Deposit {
    pub amount: Decimal,
    pub dispute: DisputeState,
}

impl From<BalanceChange> for Deposit {
    fn from(value: BalanceChange) -> Self {
        Self {
            amount: value.amount,
            dispute: DisputeState::None,
        }
    }
}

/// Dispute transaction
#[derive(Debug, Clone)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Dispute {
    pub client: u16,
    pub tx: u32,
}

/// Balance change for deposit and withdrawal
#[derive(Debug, Clone)]
pub struct BalanceChange {
    pub client: u16,
    pub tx: u32,
    pub amount: Decimal,
}

// We have to manually implement, since `Decimal` does not implement [arbitrary::Arbitrary]
#[cfg(feature = "arbitrary")]
impl arbitrary::Arbitrary<'_> for BalanceChange {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        Ok(Self {
            client: u.arbitrary()?,
            tx: u.arbitrary()?,
            amount: Decimal::from(u.arbitrary::<u16>()?),
        })
    }
}

/// A type of a parsed transaction in csv.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub enum TxType {
    Deposit(BalanceChange),
    Withdrawal(BalanceChange),
    Dispute(Dispute),
    Resolve(Dispute),
    Chargeback(Dispute),
}

/// State of a client account.
#[derive(Debug, Default, Clone, Serialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ClientAccount {
    /// The total funds that are available for trading, staking, withdrawal, etc.
    /// This should be equal to the total - held amounts
    #[serde(with = "rust_decimal::serde::str")]
    pub available: Decimal,
    /// The total funds that are held for dispute.
    /// This should be equal to total - available amounts
    #[serde(with = "rust_decimal::serde::str")]
    pub held: Decimal,
    /// The total funds that are available or held.
    /// This should be equal to available + held
    #[serde(with = "rust_decimal::serde::str")]
    pub total: Decimal,
    /// Whether the account is locked. An account is locked if a charge back occurs
    pub locked: bool,
    #[serde(skip)]
    pub deposit_txs: HashMap<u32, Deposit>,
}

impl ClientAccount {
    /// Normalise amounts to 4dp
    pub fn normalize(&mut self) {
        self.available = self.available.round_dp(4);
        self.held = self.held.round_dp(4);
        self.total = self.total.round_dp(4);
    }
}

// We need this because internal tagging does not work with csv:
// https://github.com/BurntSushi/rust-csv/issues/211
#[derive(Debug, Deserialize)]
pub struct CsvRow {
    #[serde(rename = "type")]
    tx_type: String,
    client: u16,
    tx: u32,
    amount: Option<Decimal>,
}

impl TryFrom<CsvRow> for TxType {
    type Error = anyhow::Error;

    fn try_from(row: CsvRow) -> Result<Self, Self::Error> {
        match row.tx_type.as_str() {
            "deposit" => Ok(TxType::Deposit(BalanceChange {
                client: row.client,
                tx: row.tx,
                amount: row
                    .amount
                    .ok_or(anyhow::anyhow!("Missing amount for deposit"))?,
            })),
            "withdrawal" => Ok(TxType::Withdrawal(BalanceChange {
                client: row.client,
                tx: row.tx,
                amount: row
                    .amount
                    .ok_or(anyhow::anyhow!("Missing amount for withdrawal"))?,
            })),
            "dispute" => Ok(TxType::Dispute(Dispute {
                client: row.client,
                tx: row.tx,
            })),
            "resolve" => Ok(TxType::Resolve(Dispute {
                client: row.client,
                tx: row.tx,
            })),
            "chargeback" => Ok(TxType::Chargeback(Dispute {
                client: row.client,
                tx: row.tx,
            })),
            _ => Err(anyhow::anyhow!("Unknown transaction type: {}", row.tx_type)),
        }
    }
}

/// Account row for csv output.
/// We need this explicitly since `flatten` does not work.
/// https://github.com/BurntSushi/rust-csv/issues/239
#[derive(Debug, Serialize)]
pub struct AccountRow {
    pub client: u16,
    #[serde(with = "rust_decimal::serde::str")]
    pub available: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub held: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total: Decimal,
    pub locked: bool,
}
