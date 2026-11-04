use fluxy_lib::notify::{Notifier, ShellLabels};

fn labels() -> ShellLabels {
    ShellLabels {
        open: "Open Fluxy".into(),
        quit: "Quit".into(),
        auto_clipboard: "Auto send clipboard".into(),
        autostart: "Start at login".into(),
        help: "Help".into(),
        chat_title: "New message".into(),
        chat_body: "Message from {name}".into(),
        clipboard_title: "Clipboard offer".into(),
        clipboard_body: "Clipboard offer from {name}".into(),
        clipboard_applied_title: "Clipboard received".into(),
        clipboard_applied_body: "Clipboard from {name} was applied".into(),
        transfer_title: "Transfer complete".into(),
        transfer_body: "{file} from {name}".into(),
        transfer_failed_title: "Transfer failed".into(),
        transfer_failed_body: "{file} to {name} failed".into(),
    }
}

#[test]
fn native_labels_use_the_camel_case_contract_and_are_sanitized() {
    let mut value = serde_json::to_value(labels()).expect("serialize labels");
    value["chatBody"] = serde_json::Value::String("from {name}\nsecond line".into());
    let parsed: ShellLabels = serde_json::from_value(value).expect("deserialize labels");

    let notifier = Notifier::default();
    notifier.set_labels(parsed);
    assert_eq!(notifier.labels().chat_body, "from {name}second line");
    assert_eq!(
        notifier.labels().clipboard_body,
        "Clipboard offer from {name}"
    );
    // The tray items travel through the same contract and are sanitized too.
    assert_eq!(notifier.labels().auto_clipboard, "Auto send clipboard");
    assert_eq!(notifier.labels().autostart, "Start at login");
    assert_eq!(notifier.labels().help, "Help");
}
