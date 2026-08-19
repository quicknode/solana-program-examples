use {
    crate::{
        cpi::{InitializeInstruction, SwitchPowerInstruction},
        state::{PowerStatus, PowerStatusData},
    },
    quasar_lang::{client::DynString, prelude::PodBool},
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);

#[quasar_test]
fn initialize_creates_the_power_status_switched_off(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let power = test.derive_pda(PowerStatus::seeds());

    // The power PDA and system program are canonical derivations, so the
    // generated instruction only asks for the payer.
    test.send(InitializeInstruction { payer: PAYER }).succeeds();

    let account = test.account(power).unwrap();
    assert_eq!(account.data.len(), 2, "discriminator + is_on");
    let state = test.read::<PowerStatus>(power);
    assert!(!bool::from(state.is_on), "power should be off initially");
}

#[quasar_test]
fn switch_power_turns_the_power_on(test: &mut Test) {
    let power = test.derive_pda(PowerStatus::seeds());
    // Start with power off.
    test.write(
        power,
        PowerStatusData {
            is_on: PodBool::from(false),
        },
    );

    let outcome = test.send(SwitchPowerInstruction {
        power,
        name: DynString::new("Alice"),
    });
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(
        logs.contains("pulling the power switch"),
        "should log switch"
    );
    assert!(logs.contains("now on"), "should say power is on");
    // Verifies wire format: a stale u32 length prefix would corrupt the
    // deserialised name (e.g. "\0\0\0Al" instead of "Alice").
    assert!(
        logs.contains("Alice"),
        "deserialised name should round-trip exactly; logs: {logs}"
    );

    let state = test.read::<PowerStatus>(power);
    assert!(bool::from(state.is_on), "power should now be on");
}

#[quasar_test]
fn switch_power_turns_the_power_off(test: &mut Test) {
    let power = test.derive_pda(PowerStatus::seeds());
    // Start with power on.
    test.write(
        power,
        PowerStatusData {
            is_on: PodBool::from(true),
        },
    );

    let outcome = test.send(SwitchPowerInstruction {
        power,
        name: DynString::new("Bob"),
    });
    outcome.succeeds();

    let logs = outcome.logs().join("\n");
    assert!(logs.contains("now off"), "should say power is off");
    assert!(
        logs.contains("Bob"),
        "deserialised name should round-trip exactly; logs: {logs}"
    );

    let state = test.read::<PowerStatus>(power);
    assert!(!bool::from(state.is_on), "power should now be off");
}
