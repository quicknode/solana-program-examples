use {crate::cpi::PullLeverInstruction, quasar_lang::client::DynString, quasar_test::prelude::*};

/// PowerStatus discriminator from the lever program.
const POWER_STATUS_DISCRIMINATOR: u8 = 1;

/// The lever program's ID as a test-harness Pubkey.
fn lever_program_id() -> Pubkey {
    Pubkey::from(crate::LEVER_PROGRAM_ID)
}

/// Load the lever program next to the hand program. quasar-test only
/// auto-loads sibling `.so` files from this project's own target/deploy
/// directory, so the lever ELF is read from the lever project at runtime.
fn load_lever(test: &mut Test) -> Pubkey {
    test.add(Program::new(
        lever_program_id(),
        &std::fs::read("../lever/target/deploy/quasar_lever.so").unwrap(),
    ));
    Pubkey::find_program_address(&[b"power"], &lever_program_id()).0
}

/// Install the lever's power account. Account data:
/// [discriminator: u8] [is_on: u8]
fn add_power_account(test: &mut Test, address: Pubkey, is_on: bool) {
    test.set_account(Account::new(
        address,
        lever_program_id(),
        1_000_000_000,
        vec![POWER_STATUS_DISCRIMINATOR, u8::from(is_on)],
    ));
}

#[quasar_test]
fn pull_lever_turns_the_power_on(test: &mut Test) {
    let power = load_lever(test);
    // Start with power off.
    add_power_account(test, power, false);

    // The lever program account is a canonical derivation
    // (Program<LeverProgram>), so the generated instruction only asks for
    // the power account and the name.
    let outcome = test.send(PullLeverInstruction {
        power,
        name: DynString::new("Alice"),
    });
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("Hand is pulling"), "hand should log");
    assert!(
        logs.contains("pulling the power switch"),
        "lever should log"
    );
    assert!(logs.contains("now on"), "power should turn on");
    // Verifies the CPI wire format: the lever logs the name it
    // deserialised. A stale u32 length prefix on either the inbound
    // `pull_lever` payload or the CPI to `switch_power` would corrupt
    // this (e.g. "\0\0\0Al" instead of "Alice").
    assert!(
        logs.contains("Alice"),
        "name should round-trip through hand -> lever CPI; logs: {logs}"
    );

    let account = test.account(power).unwrap();
    assert_eq!(account.data[1], 1, "power should be on");
}

#[quasar_test]
fn pull_lever_turns_the_power_off(test: &mut Test) {
    let power = load_lever(test);
    // Start with power on.
    add_power_account(test, power, true);

    let outcome = test.send(PullLeverInstruction {
        power,
        name: DynString::new("Bob"),
    });
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("now off"), "power should turn off");
    assert!(
        logs.contains("Bob"),
        "name should round-trip through hand -> lever CPI; logs: {logs}"
    );

    let account = test.account(power).unwrap();
    assert_eq!(account.data[1], 0, "power should be off");
}
