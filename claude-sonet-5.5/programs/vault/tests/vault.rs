//! LiteSVM end-to-end test. Build the program first: `cargo build-sbf`.
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
const LOCK: i64 = 60;

fn pid() -> Address {
    Address::new_from_array(vault::ID.to_bytes())
}
fn vault_pda(owner: &Address) -> Address {
    Address::find_program_address(&[b"vault", owner.as_ref()], &pid()).0
}
/// Build an instruction signed by `signer`, targeting `vault_owner`'s vault PDA.
fn ix(signer: &Address, vault_owner: &Address, data: Vec<u8>) -> Instruction {
    Instruction {
        program_id: pid(),
        accounts: vec![
            AccountMeta::new(*signer, true),
            AccountMeta::new(vault_pda(vault_owner), false),
            AccountMeta::new_readonly(Address::default(), false), // system program
        ],
        data,
    }
}
fn send(svm: &mut LiteSVM, signer: &Keypair, owner: &Address, data: Vec<u8>) -> Result<(), String> {
    svm.expire_blockhash(); // let identical txs be resent
    let msg = Message::new(&[ix(&signer.pubkey(), owner, data)], Some(&signer.pubkey()));
    let tx = Transaction::new(&[signer], msg, svm.latest_blockhash());
    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{:?} {:?}", e.err, e.meta.logs))
}
fn warp(svm: &mut LiteSVM, secs: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp += secs;
    svm.set_sysvar(&clock);
}

#[test]
fn init_deposit_locked_withdraw_close() {
    let mut svm = LiteSVM::new();
    let so = std::fs::read("../../target/deploy/vault.so").expect("run `cargo build-sbf` first");
    svm.add_program(pid(), &so).unwrap();
    let user = Keypair::new();
    let me = user.pubkey();
    svm.airdrop(&me, 10 * SOL).unwrap();
    let pda = vault_pda(&me);

    let init = vault::instruction::Init { locked_duration: LOCK }.data();
    let deposit = vault::instruction::Deposit { amount: 2 * SOL }.data();
    let withdraw = vault::instruction::Withdraw {}.data();
    let close = vault::instruction::Close {}.data();

    send(&mut svm, &user, &me, init.clone()).unwrap();
    assert!(send(&mut svm, &user, &me, init).is_err(), "re-init must fail");
    let rent = svm.get_balance(&pda).unwrap();

    send(&mut svm, &user, &me, deposit).unwrap();
    assert_eq!(svm.get_balance(&pda).unwrap(), rent + 2 * SOL);

    // Before the lock expires: withdraw and close both fail.
    let err = send(&mut svm, &user, &me, withdraw.clone()).unwrap_err();
    assert!(err.contains("StillLocked"), "{err}");
    assert!(send(&mut svm, &user, &me, close.clone()).is_err(), "close with funds");

    // Another signer cannot withdraw from this vault.
    let thief = Keypair::new();
    svm.airdrop(&thief.pubkey(), SOL).unwrap();
    warp(&mut svm, LOCK + 1);
    assert!(send(&mut svm, &thief, &me, withdraw.clone()).is_err(), "thief withdraw");
    assert!(send(&mut svm, &thief, &me, close.clone()).is_err(), "thief close");
    assert_eq!(svm.get_balance(&pda).unwrap(), rent + 2 * SOL);

    // After the lock: owner withdraws everything.
    let before = svm.get_balance(&me).unwrap();
    send(&mut svm, &user, &me, withdraw.clone()).unwrap();
    assert_eq!(svm.get_balance(&pda).unwrap(), rent);
    assert!(svm.get_balance(&me).unwrap() >= before + 2 * SOL - 10_000);
    assert!(send(&mut svm, &user, &me, withdraw).is_err(), "nothing left");

    // Close refunds rent and removes the account.
    send(&mut svm, &user, &me, close).unwrap();
    assert!(svm.get_account(&pda).map_or(true, |a| a.lamports == 0));
}
