use enum_iterator::all;

use super::CLIAgent;

#[test]
fn cli_agent_names_round_trip_historical_schema() {
    let expected = [
        (CLIAgent::Claude, "Claude"),
        (CLIAgent::Gemini, "Gemini"),
        (CLIAgent::Codex, "Codex"),
        (CLIAgent::Amp, "Amp"),
        (CLIAgent::Droid, "Droid"),
        (CLIAgent::OpenCode, "OpenCode"),
        (CLIAgent::Copilot, "Copilot"),
        (CLIAgent::Pi, "Pi"),
        (CLIAgent::OhMyPi, "OhMyPi"),
        (CLIAgent::Auggie, "Auggie"),
        (CLIAgent::CursorCli, "CursorCli"),
        (CLIAgent::Goose, "Goose"),
        (CLIAgent::Hermes, "Hermes"),
        (CLIAgent::Vibe, "Vibe"),
        (CLIAgent::Antigravity, "Antigravity"),
        (CLIAgent::WarpTui, "WarpTui"),
        (CLIAgent::Unknown, "Unknown"),
    ];

    assert_eq!(all::<CLIAgent>().count(), expected.len());
    for (agent, name) in expected {
        assert_eq!(agent.to_serialized_name(), name);
        assert_eq!(CLIAgent::from_serialized_name(name), agent);
    }
}

#[test]
fn unknown_historical_cli_agent_name_stays_decodable() {
    assert_eq!(
        CLIAgent::from_serialized_name("FutureAgent"),
        CLIAgent::Unknown
    );
}
