// M28: DDL parser (recursive descent).
//
// Consumes the token stream from `lexer.rs` and produces a `DdlDoc` AST. The
// AST types are deliberately kept local (not the serde-bound `schema.rs`
// types) so the parser stays independently testable; `codegen.rs` maps them
// onto the engine's `TopologySchema`.
//
// Guard expressions (`when <expr>`) are captured verbatim by slicing the
// original source between the first guard token and the terminator (`{`), so
// they pass through to the guard engine exactly as the user wrote them.

use super::lexer::Token;
use super::TokenKind;

/// A parsed DDL document: one signal declaration per `signal` block plus the
/// cross-signal `reaction` blocks that follow.
#[derive(Debug, Clone, PartialEq)]
pub struct DdlDoc {
    /// The declared signals, in source order.
    pub signals: Vec<SignalDecl>,
    /// The declared reactions, in source order.
    pub reactions: Vec<ReactionDecl>,
}

/// A single `signal` declaration: its id, state space, initial state and the
/// transitions out of it.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalDecl {
    /// The signal's unique id.
    pub id: String,
    /// The full set of states the signal may occupy.
    pub states: Vec<String>,
    /// The state the signal starts in (a member of `states`).
    pub initial: String,
    /// The transitions declared under this signal, in source order.
    pub transitions: Vec<TransDecl>,
}

/// One `on <event> from <src> -> <tgt>` clause, optionally guarded and bound to
/// lifecycle actions.
#[derive(Debug, Clone, PartialEq)]
pub struct TransDecl {
    /// The event name that triggers this transition.
    pub event: String,
    /// The source state, or `*` for the wildcard that matches any state.
    pub from: String,
    /// The target state.
    pub to: String,
    /// An optional guard expression; the transition is blocked on `false`.
    pub guard: Option<String>,
    /// The lifecycle actions bound to this transition. Defaults to empty.
    pub actions: DdlActionBinding,
}

/// The lifecycle actions bound to a transition, in the order the engine runs
/// them: `on_exit` → `on_transition` → `on_enter`. Each phase binds zero or
/// more action ids (comma-separated in the source).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DdlActionBinding {
    /// Actions run before leaving the source state.
    pub on_exit: Vec<String>,
    /// Actions run after tentatively entering the target state.
    pub on_transition: Vec<String>,
    /// Actions run after the transition has committed.
    pub on_enter: Vec<String>,
}

impl DdlActionBinding {
    /// `true` when no phase binds any action.
    pub fn is_empty(&self) -> bool {
        self.on_exit.is_empty() && self.on_transition.is_empty() && self.on_enter.is_empty()
    }
}

/// A single `reaction` declaration: when `from_signal` enters `from_state`,
/// deliver `event` to `to_signal`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionDecl {
    /// The signal whose state change triggers the cascade.
    pub from_signal: String,
    /// The state that triggers the cascade.
    pub from_state: String,
    /// The signal that receives the derived event.
    pub to_signal: String,
    /// The event delivered to the target signal.
    pub event: String,
    /// An optional guard expression; the reaction is skipped on `false`.
    pub guard: Option<String>,
    /// The raw source text of an optional `with { ... }` static payload block,
    /// e.g. `{ "auto": true }`. `None` when the reaction carries no payload.
    pub payload: Option<String>,
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    src: &'a str,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token], src: &'a str) -> Self {
        Parser {
            tokens,
            pos: 0,
            src,
        }
    }

    fn peek(&self) -> &'a Token {
        &self.tokens[self.pos]
    }

    fn at_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn advance(&mut self) -> &'a Token {
        let tok = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<&'a Token, String> {
        let tok = self.peek();
        if &tok.kind == kind {
            Ok(self.advance())
        } else {
            Err(format!(
                "line {} col {}: expected {:?}, found {:?}",
                tok.line, tok.col, kind, tok.kind
            ))
        }
    }

    /// Match any identifier token, returning its payload. Used where the name
    /// itself is arbitrary (signal/state/event ids).
    fn expect_any_ident(&mut self) -> Result<(String, &'a Token), String> {
        let tok = self.peek();
        match &tok.kind {
            TokenKind::Identifier(s) => {
                let s = s.clone();
                Ok((s, self.advance()))
            }
            _ => Err(format!(
                "line {} col {}: expected identifier, found {:?}",
                tok.line, tok.col, tok.kind
            )),
        }
    }

    fn expect_keyword(&mut self, kw: TokenKind) -> Result<(), String> {
        self.expect(&kw)?;
        Ok(())
    }

    fn parse_doc(&mut self) -> Result<DdlDoc, String> {
        let mut signals = Vec::new();
        let mut reactions = Vec::new();

        let mut seen_signals = std::collections::HashSet::new();

        while !self.at_end() {
            match self.peek().kind {
                TokenKind::Signal => signals.push(self.parse_signal(&mut seen_signals)?),
                TokenKind::Reaction => reactions.push(self.parse_reaction()?),
                _ => {
                    let t = self.peek();
                    return Err(format!(
                        "line {} col {}: expected 'signal' or 'reaction', found {:?}",
                        t.line, t.col, t.kind
                    ));
                }
            }
        }

        Ok(DdlDoc { signals, reactions })
    }

    fn parse_signal(&mut self, seen: &mut std::collections::HashSet<String>) -> Result<SignalDecl, String> {
        self.expect_keyword(TokenKind::Signal)?;

        let (id, id_tok) = self.expect_any_ident()?;
        if !seen.insert(id.clone()) {
            return Err(format!(
                "line {} col {}: duplicate signal '{}'",
                id_tok.line, id_tok.col, id
            ));
        }

        self.expect_keyword(TokenKind::LBrace)?;

        // `states: [...]`
        self.expect_keyword(TokenKind::States)?;
        self.expect_keyword(TokenKind::Colon)?;
        let states = self.parse_state_list()?;

        // `initial: <state>`
        self.expect_keyword(TokenKind::Initial)?;
        self.expect_keyword(TokenKind::Colon)?;
        let (initial, init_tok) = self.expect_any_ident()?;
        if !states.contains(&initial) {
            return Err(format!(
                "line {} col {}: initial state '{}' is not in the states list",
                init_tok.line, init_tok.col, initial
            ));
        }

        // Zero or more `on ...` transitions.
        let mut transitions = Vec::new();
        while matches!(self.peek().kind, TokenKind::On) {
            transitions.push(self.parse_transition(&id, &states)?);
        }

        self.expect_keyword(TokenKind::RBrace)?;

        Ok(SignalDecl {
            id,
            states,
            initial,
            transitions,
        })
    }

    fn parse_state_list(&mut self) -> Result<Vec<String>, String> {
        self.expect_keyword(TokenKind::LBracket)?;
        let mut states = Vec::new();
        // Allow an empty list, though it's not very useful.
        if !matches!(self.peek().kind, TokenKind::RBracket) {
            let (state, _) = self.expect_any_ident()?;
            states.push(state);
            while matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
                // Accept a trailing comma before `]`.
                if matches!(self.peek().kind, TokenKind::RBracket) {
                    break;
                }
                let (state, _) = self.expect_any_ident()?;
                states.push(state);
            }
        }
        self.expect_keyword(TokenKind::RBracket)?;
        Ok(states)
    }

    fn parse_transition(
        &mut self,
        signal_id: &str,
        states: &[String],
    ) -> Result<TransDecl, String> {
        self.expect_keyword(TokenKind::On)?;

        let (event, _) = self.expect_any_ident()?;

        self.expect_keyword(TokenKind::From)?;
        // M34: `from *` is an explicit wildcard that lowers to one transition
        // per source state (including the `to -> to` self-loop). The lexer
        // emits `*` as a `Mul` token, so handle it here rather than via
        // `expect_any_ident` (which would reject `*`).
        let from = if matches!(self.peek().kind, TokenKind::Mul) {
            self.advance();
            "*".to_string()
        } else {
            let (s, tok) = self.expect_any_ident()?;
            if !states.contains(&s) {
                return Err(format!(
                    "line {} col {}: 'from' state '{}' is not in the states list for '{}'",
                    tok.line, tok.col, s, signal_id
                ));
            }
            s
        };

        self.expect_keyword(TokenKind::Arrow)?;

        let (to, to_tok) = self.expect_any_ident()?;
        if !states.contains(&to) {
            return Err(format!(
                "line {} col {}: 'to' state '{}' is not in the states list for '{}'",
                to_tok.line, to_tok.col, to, signal_id
            ));
        }

        // Optional `when <guard>`.
        let guard = if matches!(self.peek().kind, TokenKind::When) {
            Some(self.parse_guard()?)
        } else {
            None
        };

        // Optional lifecycle action block. A bare `on ev from a -> b` with no
        // actions is allowed; otherwise the block is `{ ... }`.
        let actions = if matches!(self.peek().kind, TokenKind::LBrace) {
            self.advance();
            let actions = self.parse_lifecycle_actions()?;
            self.expect_keyword(TokenKind::RBrace)?;
            actions
        } else {
            DdlActionBinding::default()
        };

        Ok(TransDecl {
            event,
            from,
            to,
            guard,
            actions,
        })
    }

    /// Capture the guard expression verbatim from the source. The guard runs
    /// from the token after `when` up to (but not including) the terminator,
    /// which is `{` for transitions and `{`/`}` for reactions.
    fn parse_guard(&mut self) -> Result<String, String> {
        self.expect_keyword(TokenKind::When)?;

        // The guard must contain at least one token before the terminator.
        // Terminators are structural tokens that can legally follow a guard:
        // the `{` of an action block, the `}` of a reaction, EOF, and the `with`
        // keyword that opens a reaction's optional static payload block.
        let first_tok = self.peek();
        match first_tok.kind {
            TokenKind::LBrace | TokenKind::RBrace | TokenKind::Eof | TokenKind::With => {
                return Err(format!(
                    "line {} col {}: 'when' requires a guard expression",
                    first_tok.line, first_tok.col
                ));
            }
            _ => {}
        }

        let start_idx = self.pos;
        let mut end_idx = self.pos;

        loop {
            match self.peek().kind {
                TokenKind::LBrace | TokenKind::RBrace | TokenKind::Eof | TokenKind::With => break,
                _ => {
                    end_idx = self.pos;
                    self.advance();
                }
            }
        }

        let first = &self.tokens[start_idx];
        let last = &self.tokens[end_idx];
        let slice = &self.src[first.start..last.start + last.len];
        Ok(slice.trim().to_string())
    }

    fn parse_lifecycle_actions(&mut self) -> Result<DdlActionBinding, String> {
        let mut actions = DdlActionBinding::default();

        // Track which lifecycle hooks have been seen to reject duplicates.
        let mut seen_exit = false;
        let mut seen_transition = false;
        let mut seen_enter = false;

        while matches!(
            self.peek().kind,
            TokenKind::OnExit | TokenKind::OnTransition | TokenKind::OnEnter
        ) {
            let hook_tok = self.advance();
            self.expect_keyword(TokenKind::Colon)?;
            // M34: a hook binds a comma-separated list of action ids, e.g.
            // `on_transition: x, y, z`. Zero actions (`on_exit: ,`) is rejected
            // by the leading `expect_any_ident` below.
            let (first, _) = self.expect_any_ident()?;
            let mut ids = vec![first];
            while matches!(self.peek().kind, TokenKind::Comma) {
                self.advance();
                let (action, _) = self.expect_any_ident()?;
                ids.push(action);
            }

            match hook_tok.kind {
                TokenKind::OnExit => {
                    if seen_exit {
                        return Err(format!(
                            "line {} col {}: duplicate 'on_exit' hook",
                            hook_tok.line, hook_tok.col
                        ));
                    }
                    seen_exit = true;
                    actions.on_exit = ids;
                }
                TokenKind::OnTransition => {
                    if seen_transition {
                        return Err(format!(
                            "line {} col {}: duplicate 'on_transition' hook",
                            hook_tok.line, hook_tok.col
                        ));
                    }
                    seen_transition = true;
                    actions.on_transition = ids;
                }
                TokenKind::OnEnter => {
                    if seen_enter {
                        return Err(format!(
                            "line {} col {}: duplicate 'on_enter' hook",
                            hook_tok.line, hook_tok.col
                        ));
                    }
                    seen_enter = true;
                    actions.on_enter = ids;
                }
                _ => unreachable!(),
            }
        }

        Ok(actions)
    }

    fn parse_reaction(&mut self) -> Result<ReactionDecl, String> {
        self.expect_keyword(TokenKind::Reaction)?;
        self.expect_keyword(TokenKind::LBrace)?;

        self.expect_keyword(TokenKind::When)?;
        let (from_signal, _) = self.expect_any_ident()?;

        self.expect_keyword(TokenKind::Enters)?;
        let (from_state, _) = self.expect_any_ident()?;

        self.expect_keyword(TokenKind::Arrow)?;

        let (to_signal, _) = self.expect_any_ident()?;

        let (event, _) = self.expect_any_ident()?;

        // Optional `when <guard>`.
        let guard = if matches!(self.peek().kind, TokenKind::When) {
            Some(self.parse_guard()?)
        } else {
            None
        };

        // Optional static payload block: `with { ... }`. Matches the canonical
        // example's empty `{}` block (reserved, ignored) when `with` is absent.
        let payload = if matches!(self.peek().kind, TokenKind::With) {
            self.advance();
            Some(self.parse_raw_brace_block()?)
        } else if matches!(self.peek().kind, TokenKind::LBrace) {
            self.advance();
            self.expect_keyword(TokenKind::RBrace)?;
            None
        } else {
            None
        };

        self.expect_keyword(TokenKind::RBrace)?;

        Ok(ReactionDecl {
            from_signal,
            from_state,
            to_signal,
            event,
            guard,
            payload,
        })
    }

    /// Consume a `{ ... }` block (the leading `{` is the current token) and
    /// return its raw source text from the opening `{` through the matching
    /// `}` — braces nested inside are tracked by depth. Used to capture a
    /// reaction's `with { ... }` static payload verbatim for JSON parsing in
    /// codegen.
    fn parse_raw_brace_block(&mut self) -> Result<String, String> {
        let open = self.expect(&TokenKind::LBrace)?;
        let start = open.start;
        let mut depth = 1usize;
        loop {
            let tok = self.peek();
            if matches!(tok.kind, TokenKind::LBrace) {
                depth += 1;
                self.advance();
            } else if matches!(tok.kind, TokenKind::RBrace) {
                depth -= 1;
                let end = tok.start + tok.len;
                self.advance();
                if depth == 0 {
                    return Ok(self.src[start..end].to_string());
                }
            } else if matches!(tok.kind, TokenKind::Eof) {
                return Err(format!(
                    "line {} col {}: unterminated 'with' payload block",
                    tok.line, tok.col
                ));
            } else {
                self.advance();
            }
        }
    }
}

/// Parse a token stream into a `DdlDoc` AST. `src` is the original source text,
/// used to slice guard expressions verbatim.
pub fn parse(tokens: &[Token], src: &str) -> Result<DdlDoc, String> {
    let mut parser = Parser::new(tokens, src);
    let doc = parser.parse_doc()?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddl::lexer::tokenize;

    fn src_to_doc(src: &str) -> Result<DdlDoc, String> {
        let tokens = tokenize(src).unwrap();
        parse(&tokens, src)
    }

    #[test]
    fn parse_single_signal_no_actions() {
        let doc = src_to_doc(
            r#"
signal task {
    states: [idle, running, done]
    initial: idle

    on start from idle -> running
    on finish from running -> done
}
"#,
        )
        .unwrap();

        assert_eq!(doc.signals.len(), 1);
        let sig = &doc.signals[0];
        assert_eq!(sig.id, "task");
        assert_eq!(sig.states, vec!["idle", "running", "done"]);
        assert_eq!(sig.initial, "idle");
        assert_eq!(sig.transitions.len(), 2);

        assert_eq!(sig.transitions[0].event, "start");
        assert_eq!(sig.transitions[0].from, "idle");
        assert_eq!(sig.transitions[0].to, "running");
        assert!(sig.transitions[0].guard.is_none());
        assert!(sig.transitions[0].actions.is_empty());

        assert_eq!(sig.transitions[1].event, "finish");
        assert_eq!(sig.transitions[1].to, "done");
    }

    #[test]
    fn parse_signal_with_all_lifecycle_actions_and_guard() {
        let doc = src_to_doc(
            r#"
signal order {
    states: [draft, submitted, approved]
    initial: draft

    on submit from draft -> submitted {
        on_exit: log_draft_exit
        on_transition: validate_order_payload
        on_enter: notify_submitted
    }

    on approve from submitted -> approved when payload.amount > 0 and payload.amount <= 100000 {
        on_transition: reserve_inventory
        on_enter: notify_customer_approved
    }
}
"#,
        )
        .unwrap();

        let order = &doc.signals[0];
        assert_eq!(order.transitions.len(), 2);

        let submit = &order.transitions[0];
        assert_eq!(submit.actions.on_exit, vec!["log_draft_exit"]);
        assert_eq!(submit.actions.on_transition, vec!["validate_order_payload"]);
        assert_eq!(submit.actions.on_enter, vec!["notify_submitted"]);

        let approve = &order.transitions[1];
        assert_eq!(
            approve.guard,
            Some("payload.amount > 0 and payload.amount <= 100000".to_string())
        );
        assert_eq!(approve.actions.on_transition, vec!["reserve_inventory"]);
        assert_eq!(approve.actions.on_enter, vec!["notify_customer_approved"]);
    }

    #[test]
    fn parse_reaction() {
        let doc = src_to_doc(
            r#"
reaction {
    when order enters approved -> order_fulfill begin
}
"#,
        )
        .unwrap();

        assert_eq!(doc.reactions.len(), 1);
        let r = &doc.reactions[0];
        assert_eq!(r.from_signal, "order");
        assert_eq!(r.from_state, "approved");
        assert_eq!(r.to_signal, "order_fulfill");
        assert_eq!(r.event, "begin");
        assert!(r.guard.is_none());
    }

    #[test]
    fn parse_reaction_with_guard_and_payload_block() {
        let doc = src_to_doc(
            r#"
reaction {
    when order enters approved -> order_fulfill begin when payload.auto {
    }
}
"#,
        )
        .unwrap();

        let r = &doc.reactions[0];
        assert_eq!(r.guard, Some("payload.auto".to_string()));
    }

    #[test]
    fn missing_arrow_reports_location() {
        let err = src_to_doc(
            r#"
signal s {
    states: [a, b]
    initial: a

    on go from a b
}
"#,
        )
        .unwrap_err();
        assert!(err.contains("line 6"), "got: {}", err);
        assert!(err.contains("expected"), "got: {}", err);
    }

    #[test]
    fn unknown_top_level_keyword_reports_location() {
        let err = src_to_doc("bogus {}").unwrap_err();
        assert!(err.contains("line 1"), "got: {}", err);
        assert!(err.contains("expected 'signal' or 'reaction'"), "got: {}", err);
    }

    #[test]
    fn duplicate_signal_reports_location() {
        let err = src_to_doc(
            r#"
signal dup {
    states: [a]
    initial: a
}
signal dup {
    states: [b]
    initial: b
}
"#,
        )
        .unwrap_err();
        assert!(err.contains("duplicate signal 'dup'"), "got: {}", err);
    }

    #[test]
    fn missing_states_reports_error() {
        // No `states:` line -> parser expects it after the `{`.
        let err = src_to_doc(
            r#"
signal s {
    initial: a
}
"#,
        )
        .unwrap_err();
        assert!(err.contains("line 3"), "got: {}", err);
    }

    #[test]
    fn initial_not_in_states_reports_error() {
        let err = src_to_doc(
            r#"
signal s {
    states: [a, b]
    initial: c
}
"#,
        )
        .unwrap_err();
        assert!(err.contains("initial state 'c'"), "got: {}", err);
    }

    #[test]
    fn from_state_not_in_list_reports_error() {
        let err = src_to_doc(
            r#"
signal s {
    states: [a, b]
    initial: a

    on go from z -> b
}
"#,
        )
        .unwrap_err();
        assert!(err.contains("'from' state 'z'"), "got: {}", err);
    }

    #[test]
    fn parse_wildcard_from() {
        let doc = src_to_doc(
            r#"
signal s {
    states: [a, b, c]
    initial: a

    on reset from * -> a
}
"#,
        )
        .unwrap();

        let s = &doc.signals[0];
        assert_eq!(s.transitions.len(), 1);
        assert_eq!(s.transitions[0].from, "*");
        assert_eq!(s.transitions[0].to, "a");
        assert_eq!(s.transitions[0].event, "reset");
    }

    #[test]
    fn parse_multi_action_hooks_preserve_order() {
        let doc = src_to_doc(
            r#"
signal s {
    states: [a, b]
    initial: a

    on go from a -> b {
        on_exit: e1, e2
        on_transition: t1, t2, t3
        on_enter: n1
    }
}
"#,
        )
        .unwrap();

        let tr = &doc.signals[0].transitions[0];
        assert_eq!(tr.actions.on_exit, vec!["e1", "e2"]);
        assert_eq!(tr.actions.on_transition, vec!["t1", "t2", "t3"]);
        assert_eq!(tr.actions.on_enter, vec!["n1"]);
    }

    #[test]
    fn parse_zero_action_hook_is_rejected() {
        // An `on_exit:` with no following identifier is a parse error.
        let err = src_to_doc(
            r#"
signal s {
    states: [a, b]
    initial: a

    on go from a -> b {
        on_exit: ,
    }
}
"#,
        )
        .unwrap_err();
        assert!(err.contains("expected identifier"), "got: {}", err);
    }

    #[test]
    fn parse_reaction_with_payload() {
        let doc = src_to_doc(
            r#"
reaction {
    when order enters approved -> inventory allocate when true
    with { "auto": true, "count": 1 }
}
"#,
        )
        .unwrap();

        let r = &doc.reactions[0];
        assert_eq!(r.guard, Some("true".to_string()));
        assert_eq!(r.payload, Some(r#"{ "auto": true, "count": 1 }"#.to_string()));
    }

    #[test]
    fn parse_reaction_without_payload_still_works() {
        let doc = src_to_doc(
            r#"
reaction {
    when order enters approved -> inventory allocate when true
}
"#,
        )
        .unwrap();

        let r = &doc.reactions[0];
        assert!(r.payload.is_none());
    }

    #[test]
    fn empty_guard_reports_error() {
        let err = src_to_doc(
            r#"
signal s {
    states: [a, b]
    initial: a

    on go from a -> b when {
    }
}
"#,
        )
        .unwrap_err();
        assert!(err.contains("requires a guard expression"), "got: {}", err);
    }
}
