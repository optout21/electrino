use bip157_store::BlockHash;
use bitcoin::params::Params;
use bitcoin::{Address, Amount, OutPoint, ScriptBuf, Txid};

use std::collections::{HashMap, HashSet};
use std::fmt;

fn script_addr_info(script: &ScriptBuf) -> String {
    match Address::from_script(script, Params::BITCOIN) {
        Ok(addr) => addr.to_string(),
        Err(_) => script.to_string(),
    }
}

/// Block identification: height, hash, and timestamp.
#[derive(Debug, Clone)]
pub struct BlockInfo {
    /// The block height.
    pub height: u32,
    /// The block hash.
    pub hash: BlockHash,
    // /// The Unix timestamp of the block.
    // pub time: u32,
}

impl BlockInfo {
    pub fn new(height: u32, hash: BlockHash) -> Self {
        Self { height, hash }
    }
}

impl fmt::Display for BlockInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "height={} hash={}", self.height, self.hash)
    }
}

/// Information about the block in which a transaction output was spent.
#[derive(Debug, Clone)]
pub struct SpendingInfo {
    /// The transaction in which the Txo was spent
    pub txid: Txid,
    pub input_index: u32,
    /// The block in which the spending transaction was confirmed.
    pub block: BlockInfo,
}

impl fmt::Display for SpendingInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "spent in {} ({}) block {}",
            self.txid, self.input_index, self.block
        )
    }
}

/// Information about an unspent transaction output.
#[derive(Debug, Clone)]
pub struct TxoInfo {
    pub script: ScriptBuf,
    /// The block in which the transaction that created this output was confirmed.
    pub block: BlockInfo,
    /// The output reference (transaction ID and index).
    pub output: OutPoint,
    pub amount: Amount,
    /// Spending information, if this output has been spent.
    pub spent: Option<SpendingInfo>,
}

impl TxoInfo {
    pub fn is_spent(&self) -> bool {
        self.spent.is_some()
    }
}

impl fmt::Display for TxoInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}:{} block({}) {}",
            script_addr_info(&self.script),
            self.amount,
            self.output.txid,
            self.output.vout,
            self.block,
            match &self.spent {
                Some(s) => s.to_string(),
                None => "unspent".to_string(),
            }
        )
    }
}

/// The UTXO snapshot of a script
#[derive(Debug, Clone, Default)]
pub struct ScriptUtxoSnapshot {
    /// The script
    pub script: ScriptBuf,
    /// The unspent amount
    pub total_unspent: Amount,
    /// The total received amount
    pub total_received: Amount,
    /// The UTXOs
    pub utxos: Vec<TxoInfo>,
    /// Number of spent TXOs
    pub stxo_count: usize,
    /// The STXOs
    pub stxos: Option<Vec<TxoInfo>>,
}

impl ScriptUtxoSnapshot {
    /// Print all UTXOs
    pub fn print(&self) {
        println!(
            "Script: {}:  {} UTXOs  {} STXOs  unspent {}  received {}",
            script_addr_info(&self.script),
            self.utxos.len(),
            self.stxo_count,
            self.total_unspent,
            self.total_received,
        );
        for txo in &self.utxos {
            println!("  {}", txo);
        }
        if let Some(stxos) = &self.stxos {
            for txo in stxos {
                println!("  {}", txo);
            }
        }
    }
}

/// A set of UTXOs across multiple scripts.
#[derive(Debug, Clone, Default)]
pub struct TxoSet {
    /// The UTXOs, indexed by output reference.
    utxos: HashMap<OutPoint, TxoInfo>,
}

impl TxoSet {
    pub fn new() -> Self {
        Self {
            utxos: HashMap::new(),
        }
    }

    /// Add a new unspent output to the set.
    pub fn add(
        &mut self,
        script: ScriptBuf,
        block: BlockInfo,
        output: OutPoint,
        amount: Amount,
    ) -> TxoInfo {
        let txo = TxoInfo {
            script,
            block,
            output,
            amount,
            spent: None,
        };
        self.utxos.insert(output, txo.clone());
        txo
    }

    /// Mark an output as spent in the given block. Returns TxInfo (if found), and if it was changed
    pub fn set_spent(
        &mut self,
        output: OutPoint,
        txid: Txid,
        input_index: u32,
        block: BlockInfo,
    ) -> Option<(TxoInfo, bool)> {
        if let Some(utxo) = self.utxos.get_mut(&output) {
            if !utxo.is_spent() {
                utxo.spent = Some(SpendingInfo {
                    txid,
                    input_index,
                    block,
                });
                Some((utxo.clone(), true))
            } else {
                Some((utxo.clone(), false))
            }
        } else {
            None
        }
    }

    pub fn get_script_snaphshot(
        &self,
        script: &ScriptBuf,
        include_stxos: bool,
    ) -> ScriptUtxoSnapshot {
        let mut total_unspent = 0u64;
        let mut total_received = 0u64;
        let mut utxos = Vec::new();
        let mut stxo_count = 0;
        let mut stxos = Vec::new();
        for txo in self.utxos.values() {
            if txo.script == *script {
                total_received += txo.amount.to_sat();
                if !txo.is_spent() {
                    total_unspent += txo.amount.to_sat();
                    utxos.push(txo.clone());
                } else {
                    stxo_count += 1;
                    if include_stxos {
                        stxos.push(txo.clone());
                    }
                }
            }
        }
        ScriptUtxoSnapshot {
            script: script.clone(),
            total_unspent: Amount::from_sat(total_unspent),
            total_received: Amount::from_sat(total_received),
            utxos,
            stxo_count,
            stxos: if include_stxos { Some(stxos) } else { None },
        }
    }

    pub fn get_scripts(&self) -> Vec<ScriptBuf> {
        let mut set = HashSet::new();
        for txo in self.utxos.values() {
            if !set.contains(&txo.script) {
                set.insert(txo.script.clone());
            }
        }
        set.into_iter().collect()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.utxos.len()
    }

    /// Print all UTXOs
    pub fn print(&self, include_spent: bool) {
        let scripts = self.get_scripts();
        for script in scripts {
            let ss = self.get_script_snaphshot(&script, include_spent);
            ss.print();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn block(height: u32) -> BlockInfo {
        let hash =
            BlockHash::from_str("000000000000000000029a62a1c3b79a9a96a81d07e48a963e9c3be1d1d5e4d5")
                .unwrap();
        BlockInfo::new(height, hash)
    }

    fn txid(n: u8) -> Txid {
        Txid::from_str(&format!("{:0>64}", n)).unwrap()
    }

    fn outpoint(n: u8, vout: u32) -> OutPoint {
        OutPoint::new(txid(n), vout)
    }

    fn script(n: u8) -> ScriptBuf {
        // Simple OP_RETURN scripts with a distinguishing byte
        ScriptBuf::from(vec![0x6a, n])
    }

    #[test]
    fn test_add_and_get_snapshot() {
        let mut set = TxoSet::new();
        let s1 = script(1);
        let s2 = script(2);

        set.add(
            s1.clone(),
            block(100),
            outpoint(1, 0),
            Amount::from_sat(1000),
        );
        set.add(
            s1.clone(),
            block(101),
            outpoint(2, 0),
            Amount::from_sat(2000),
        );
        set.add(
            s2.clone(),
            block(102),
            outpoint(3, 0),
            Amount::from_sat(3000),
        );

        assert_eq!(set.len(), 3);

        let ss1 = set.get_script_snaphshot(&s1, true);
        assert_eq!(ss1.script, s1);
        assert_eq!(ss1.total_unspent.to_sat(), 3000);
        assert_eq!(ss1.total_received.to_sat(), 3000);
        assert_eq!(ss1.utxos.len(), 2);
        assert_eq!(ss1.stxo_count, 0);
        assert_eq!(ss1.stxos.unwrap().len(), 0);

        let ss2 = set.get_script_snaphshot(&s2, true);
        assert_eq!(ss2.script, s2);
        assert_eq!(ss2.total_unspent.to_sat(), 3000);
        assert_eq!(ss2.total_received.to_sat(), 3000);
        assert_eq!(ss2.utxos.len(), 1);
        assert_eq!(ss2.stxo_count, 0);
        assert_eq!(ss2.stxos.unwrap().len(), 0);
    }

    #[test]
    fn test_get_scripts() {
        let mut set = TxoSet::new();
        let s1 = script(1);
        let s2 = script(2);

        set.add(
            s1.clone(),
            block(100),
            outpoint(1, 0),
            Amount::from_sat(1000),
        );
        set.add(
            s1.clone(),
            block(101),
            outpoint(2, 0),
            Amount::from_sat(2000),
        );
        set.add(
            s2.clone(),
            block(102),
            outpoint(3, 0),
            Amount::from_sat(3000),
        );

        let mut scripts = set.get_scripts();
        scripts.sort();
        assert_eq!(scripts.len(), 2);
        assert!(scripts.contains(&s1));
        assert!(scripts.contains(&s2));
    }

    #[test]
    fn test_set_spent_existing() {
        let mut set = TxoSet::new();
        let s1 = script(1);
        let op = outpoint(1, 0);

        set.add(s1.clone(), block(100), op, Amount::from_sat(1000));
        let ss1 = set.get_script_snaphshot(&s1, true);
        assert_eq!(ss1.total_unspent.to_sat(), 1000);
        assert_eq!(ss1.total_received.to_sat(), 1000);
        assert_eq!(ss1.utxos.len(), 1);
        assert_eq!(ss1.stxo_count, 0);
        assert_eq!(ss1.stxos.unwrap().len(), 0);

        let result = set.set_spent(op, txid(99), 0, block(200));
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.0.is_spent());
        assert!(result.1);

        // now shows as spent, not unspent
        let ss1 = set.get_script_snaphshot(&s1, true);
        assert_eq!(ss1.total_unspent.to_sat(), 0);
        assert_eq!(ss1.total_received.to_sat(), 1000);
        assert_eq!(ss1.utxos.len(), 0);
        assert_eq!(ss1.stxo_count, 1);
        assert_eq!(ss1.stxos.unwrap().len(), 1);
    }

    #[test]
    fn test_set_spent_nonexistent() {
        let mut set = TxoSet::new();
        set.add(
            script(1),
            block(100),
            outpoint(1, 0),
            Amount::from_sat(1000),
        );

        let result = set.set_spent(outpoint(99, 0), txid(99), 0, block(200));
        assert!(result.is_none());
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_set_spent_already_spent() {
        let mut set = TxoSet::new();
        let op = outpoint(1, 0);
        set.add(script(1), block(100), op, Amount::from_sat(1000));

        set.set_spent(op, txid(10), 0, block(200));
        // second spend attempt returns None
        let result = set.set_spent(op, txid(11), 0, block(201));
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.0.is_spent());
        assert!(!result.1);
    }
}
