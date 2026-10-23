//! A sample authorization window, for `torii config theme test` and
//! `torii config fbc test`.
//!
//! The human sees a theme and a folded bar color on the real window, before or
//! after choosing them. The request is plainly a joke and the agent's reason
//! says it is a test: nothing runs and nothing is written, whatever the human
//! answers. It opens in the CLI process itself, whose stdout is not an MCP
//! transport.

use super::{gui, gui_disabled, theme, PermanentOption, PermanentPolicy};
use crate::config::appearance::{FoldedBarColor, Theme};
use crate::error::{Error, Result};

const PROVIDER: &str = "teste-de-tema";

const ARGS: [&str; 8] = [
    "pizza",
    "order",
    "--size",
    "familia",
    "--topping",
    "abacaxi",
    "--deliver-to",
    "sala-de-reuniao",
];

const DESCRIPTION: &str = "Teste de tema do Torii: esta janela não executa nem grava nada, \
     responda como quiser. O agente fictício quer pedir uma pizza família de abacaxi para a \
     sala de reunião (passo 1 de 1).";

/// The policy a permanent decision would name. The window only shows it, and
/// the decision is thrown away.
const SCOPE: &str = "política de mentirinha (teste de tema)";

/// Open the sample window with this appearance and wait for it to close.
pub fn show(theme: Theme, folded: FoldedBarColor) -> Result<()> {
    if gui_disabled() {
        return Err(Error::Prompt(
            "TORII_NO_GUI is set; the test window cannot open".into(),
        ));
    }
    theme::install(theme, folded);
    let args = ARGS.map(str::to_owned).to_vec();
    gui::access_window(
        PROVIDER.into(),
        args.clone(),
        2,
        None,
        permanent(&args),
        Some(DESCRIPTION.into()),
    )
    .map_err(Error::Prompt)?;
    Ok(())
}

/// Every boundary offers both permanent buttons, so their colors show too.
fn permanent(args: &[String]) -> PermanentPolicy {
    PermanentPolicy {
        scope: SCOPE.into(),
        options: (1..=args.len())
            .map(|count| PermanentOption {
                rule: args[..count].join(" "),
                accept_blocked: None,
                deny_blocked: None,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_says_it_is_a_test() {
        assert!(DESCRIPTION.contains("Teste de tema"));
        assert!(DESCRIPTION.contains("não executa nem grava nada"));
        assert!(SCOPE.contains("teste de tema"));
    }

    #[test]
    fn every_boundary_offers_a_permanent_rule() {
        let args = ARGS.map(str::to_owned).to_vec();
        let policy = permanent(&args);
        assert_eq!(policy.options.len(), ARGS.len());
        assert_eq!(policy.options[1].rule, "pizza order");
        assert!(policy
            .options
            .iter()
            .all(|option| option.allows_accept() && option.allows_deny()));
    }
}
