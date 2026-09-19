//! Storage access for the account table.

pub const SELECT_ACCOUNT: &str = "SELECT id, email FROM account";

pub const SELECT_AUDIT_01: &str = "SELECT id, account_id, payload FROM audit_01";
pub const SELECT_AUDIT_02: &str = "SELECT id, account_id, payload FROM audit_02";
pub const SELECT_AUDIT_03: &str = "SELECT id, account_id, payload FROM audit_03";
pub const SELECT_AUDIT_04: &str = "SELECT id, account_id, payload FROM audit_04";
pub const SELECT_AUDIT_05: &str = "SELECT id, account_id, payload FROM audit_05";
pub const SELECT_AUDIT_06: &str = "SELECT id, account_id, payload FROM audit_06";
pub const SELECT_AUDIT_07: &str = "SELECT id, account_id, payload FROM audit_07";
pub const SELECT_AUDIT_08: &str = "SELECT id, account_id, payload FROM audit_08";
pub const SELECT_AUDIT_09: &str = "SELECT id, account_id, payload FROM audit_09";
pub const SELECT_AUDIT_10: &str = "SELECT id, account_id, payload FROM audit_10";
pub const SELECT_AUDIT_11: &str = "SELECT id, account_id, payload FROM audit_11";
pub const SELECT_AUDIT_12: &str = "SELECT id, account_id, payload FROM audit_12";
pub const SELECT_AUDIT_13: &str = "SELECT id, account_id, payload FROM audit_13";
pub const SELECT_AUDIT_14: &str = "SELECT id, account_id, payload FROM audit_14";
pub const SELECT_AUDIT_15: &str = "SELECT id, account_id, payload FROM audit_15";
pub const SELECT_AUDIT_16: &str = "SELECT id, account_id, payload FROM audit_16";
pub const SELECT_AUDIT_17: &str = "SELECT id, account_id, payload FROM audit_17";
pub const SELECT_AUDIT_18: &str = "SELECT id, account_id, payload FROM audit_18";
pub const SELECT_AUDIT_19: &str = "SELECT id, account_id, payload FROM audit_19";
pub const SELECT_AUDIT_20: &str = "SELECT id, account_id, payload FROM audit_20";
pub const SELECT_AUDIT_21: &str = "SELECT id, account_id, payload FROM audit_21";
pub const SELECT_AUDIT_22: &str = "SELECT id, account_id, payload FROM audit_22";
pub const SELECT_AUDIT_23: &str = "SELECT id, account_id, payload FROM audit_23";
pub const SELECT_AUDIT_24: &str = "SELECT id, account_id, payload FROM audit_24";
pub const SELECT_AUDIT_25: &str = "SELECT id, account_id, payload FROM audit_25";
pub const SELECT_AUDIT_26: &str = "SELECT id, account_id, payload FROM audit_26";
pub const SELECT_AUDIT_27: &str = "SELECT id, account_id, payload FROM audit_27";
pub const SELECT_AUDIT_28: &str = "SELECT id, account_id, payload FROM audit_28";
pub const SELECT_AUDIT_29: &str = "SELECT id, account_id, payload FROM audit_29";
pub const SELECT_AUDIT_30: &str = "SELECT id, account_id, payload FROM audit_30";

pub fn load(conn: &Connection, id: i64) -> Option<Account> {
    conn.query(SELECT_ACCOUNT, &[id]).ok()
}
