//! End-to-end test in LiteSVM. Build the program first: `cargo build-sbf`.
use anchor_lang::InstructionData;
use litesvm::LiteSVM;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

const SOL: u64 = 1_000_000_000;

fn ix(user: &Address, data: Vec<u8>) -> Instruction {
    let pid = Address::new_from_array(vault::ID.to_bytes());
    let pda = |s: &[u8]| Address::find_program_address(&[s, user.as_ref()], &pid).0;
    let accounts = vec![
        AccountMeta::new(*user, true),
        AccountMeta::new(pda(b"state"), false),
        AccountMeta::new(pda(b"vault"), false),
        AccountMeta::new_readonly(Address::default(), false) // system program,
    ];
    Instruction { program_id: pid, accounts, data }
}
fn send(svm: &mut LiteSVM, user: &Keypair, data: Vec<u8>) -> Result<(), String> {
    svm.expire_blockhash(); // fresh blockhash so identical txs can be sent twice
    let msg = Message::new(&[ix(&user.pubkey(), data)], Some(&user.pubkey()));
    let tx = Transaction::new(&[user], msg, svm.latest_blockhash());
    svm.send_transaction(tx).map(|_| ()).map_err(|e| format!("{:?} {:?}", e.err, e.meta.logs))
}
fn warp(svm: &mut LiteSVM, secs: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp += secs;
    svm.set_sysvar(&clock);
}
#[test]
fn lock_then_withdraw_then_close() {
    let mut svm = LiteSVM::new();
    let so = std::fs::read("../../target/deploy/vault.so").expect("run `cargo build-sbf` first");
    svm.add_program(Address::new_from_array(vault::ID.to_bytes()), &so).unwrap();
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 10 * SOL).unwrap();

    send(&mut svm, &user, vault::instruction::Init { locked_duration: 60 }.data()).unwrap();
    send(&mut svm, &user, vault::instruction::Deposit { amount: 2 * SOL }.data()).unwrap();

    // Locked: withdraw and close both fail before 60s pass.
    let err = send(&mut svm, &user, vault::instruction::Withdraw { amount: SOL }.data());
    assert!(err.unwrap_err().contains("StillLocked"));
    assert!(send(&mut svm, &user, vault::instruction::Close {}.data()).is_err());

    warp(&mut svm, 61);
    let err = send(&mut svm, &user, vault::instruction::Withdraw { amount: 3 * SOL }.data());
    assert!(err.unwrap_err().contains("InsufficientFunds"));
    send(&mut svm, &user, vault::instruction::Withdraw { amount: SOL }.data()).unwrap();

    // Another user cannot touch this vault: their PDAs differ, so the state doesn't exist.
    let thief = Keypair::new();
    svm.airdrop(&thief.pubkey(), SOL).unwrap();
    assert!(send(&mut svm, &thief, vault::instruction::Withdraw { amount: SOL }.data()).is_err());

    send(&mut svm, &user, vault::instruction::Close {}.data()).unwrap();
    let balance = svm.get_balance(&user.pubkey()).unwrap();
    assert!(balance > 10 * SOL - 100_000, "all SOL + rent returned, minus fees: {balance}");
}
