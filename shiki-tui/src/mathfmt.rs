//! Renders the *content* of `$$...$$` math blocks as readable Unicode
//! instead of raw LaTeX source. A terminal can't typeset `\frac{\sqrt{\pi}}{2}`
//! the way LaTeX does, but it can do better than echoing the markup verbatim:
//! `\frac` → `a/b`, `\sqrt` → `√`, `^2` → `²`, `_0` → `₀`, `\pi` → `π`,
//! `\infty` → `∞`, `\int` → `∫` — so the math-blocks note's formula reads as
//! `∫₀^∞ e⁻ˣ² dx = √π/2` instead of `\int_0^\infty e^{-x^2} dx =
//! \frac{\sqrt{\pi}}{2}`.
//!
//! This is deliberately a *lightweight* hand-rolled converter, not a real
//! TeX engine: it handles the constructs this app's `/`-menu's `math` block
//! and common notes actually use (fractions, square roots, Greek letters,
//! sub/superscripts, common operators), and anything it doesn't recognize is
//! passed through unchanged rather than mangled — a note that drops in a
//! genuinely exotic macro still shows something, just not prettified.

use std::collections::HashMap;

/// Converts LaTeX math markup to Unicode text. Unknown commands are dropped
/// (their following argument still renders); unknown literal text passes
/// through untouched.
pub fn latex_to_unicode(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut name = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_alphabetic() {
                        name.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    // Escaped punctuation: `\,` `\;` are spacing, `\!` is a
                    // negative space (nothing), everything else is literal.
                    match chars.next() {
                        Some(',') | Some(';') => out.push(' '),
                        Some('!') => {}
                        Some(other) => out.push(other),
                        None => out.push('\\'),
                    }
                    continue;
                }
                match name.as_str() {
                    "frac" => {
                        let num = latex_to_unicode(&read_braced(&mut chars));
                        let den = latex_to_unicode(&read_braced(&mut chars));
                        out.push_str(&format!("{num}/{den}"));
                    }
                    "sqrt" => {
                        let inner = latex_to_unicode(&read_braced(&mut chars));
                        out.push('√');
                        out.push_str(&inner);
                    }
                    "text" | "mathrm" | "operatorname" | "mbox" | "mathbf" | "mathit"
                    | "textrm" => {
                        out.push_str(&latex_to_unicode(&read_braced(&mut chars)));
                    }
                    // `\left(` / `\right)` — the delimiters that follow render
                    // as themselves; the commands add nothing to display.
                    "left" | "right" | "big" | "Big" | "bigg" | "Bigg" => {}
                    "left(" | "right)" => {} // defensive, shouldn't be reached
                    "quad" | "qquad" => out.push_str("  "),
                    "space" => out.push(' '),
                    "bar" | "hat" | "vec" | "overline" | "underline" | "widetilde" | "widehat" => {
                        out.push_str(&latex_to_unicode(&read_braced(&mut chars)));
                    }
                    _ => {
                        if let Some(sym) = symbols().get(name.as_str()) {
                            out.push_str(sym);
                        }
                        // Unknown command: drop it; whatever argument follows
                        // still gets rendered on its own.
                    }
                }
            }
            '^' => {
                let arg = latex_to_unicode(&read_exponent(&mut chars));
                out.push_str(&to_script(&arg, ScriptKind::Super));
            }
            '_' => {
                let arg = latex_to_unicode(&read_exponent(&mut chars));
                out.push_str(&to_script(&arg, ScriptKind::Sub));
            }
            '{' | '}' => {} // grouping braces — nothing to display
            '$' => {}       // stray `$` — should never survive the renderer
            '~' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptKind {
    Super,
    Sub,
}

/// Reads a balanced `{...}` group, returning its raw inner text (the outer
/// braces consumed, nested braces handled). No opening `{` → empty; no
/// closing brace → the rest of the input.
fn read_braced(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    if chars.peek() != Some(&'{') {
        return String::new();
    }
    chars.next(); // consume the opening {
    let mut depth = 1usize;
    let mut inner = String::new();
    for c in chars.by_ref() {
        match c {
            '{' => {
                depth += 1;
                inner.push(c);
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
                inner.push(c);
            }
            _ => inner.push(c),
        }
    }
    inner
}

/// Reads the argument of `^`/`_`: a `{...}` group (balanced), a single
/// backslash-command (`\infty`, `\pi`), or a single character. Returns the
/// raw text for the caller to run through `latex_to_unicode`.
fn read_exponent(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    if chars.peek() == Some(&'{') {
        return read_braced(chars);
    }
    let mut s = String::new();
    if chars.peek() == Some(&'\\') {
        s.push(chars.next().unwrap());
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                s.push(c);
                chars.next();
            } else {
                break;
            }
        }
    } else if let Some(c) = chars.next() {
        s.push(c);
    }
    s
}

/// Converts a (already-LaTeX-rendered) script argument to Unicode super/sub-
/// script text. Chars with a mapping are converted; chars that are already a
/// script glyph pass through (handles nested `e^{-x^2}` → `⁻ˣ²`); anything
/// else falls back to the explicit `^arg`/`_arg` form rather than being lost.
fn to_script(arg: &str, kind: ScriptKind) -> String {
    let map: &[(char, char)] = match kind {
        ScriptKind::Super => SUPER,
        ScriptKind::Sub => SUB,
    };
    let mut all_mapped = true;
    let mut mapped = String::new();
    for c in arg.chars() {
        if let Some((_, to)) = map.iter().find(|(from, _)| *from == c) {
            mapped.push(*to);
        } else if is_script_glyph(c) {
            mapped.push(c);
        } else {
            all_mapped = false;
            break;
        }
    }
    if all_mapped && !mapped.is_empty() {
        return mapped;
    }
    match kind {
        ScriptKind::Super => format!("^{arg}"),
        ScriptKind::Sub => format!("_{arg}"),
    }
}

fn is_script_glyph(c: char) -> bool {
    SUPER.iter().any(|(_, to)| *to == c) || SUB.iter().any(|(_, to)| *to == c)
}

/// Superscript mappings for common letters/digits/symbols (lowercase only —
/// uppercase has no general superscript form, so it falls back to `^A` etc).
const SUPER: &[(char, char)] = &[
    ('0', '⁰'),
    ('1', '¹'),
    ('2', '²'),
    ('3', '³'),
    ('4', '⁴'),
    ('5', '⁵'),
    ('6', '⁶'),
    ('7', '⁷'),
    ('8', '⁸'),
    ('9', '⁹'),
    ('+', '⁺'),
    ('-', '⁻'),
    ('=', '⁼'),
    ('(', '⁽'),
    (')', '⁾'),
    ('a', 'ᵃ'),
    ('b', 'ᵇ'),
    ('c', 'ᶜ'),
    ('d', 'ᵈ'),
    ('e', 'ᵉ'),
    ('f', 'ᶠ'),
    ('g', 'ᵍ'),
    ('h', 'ʰ'),
    ('i', 'ⁱ'),
    ('j', 'ʲ'),
    ('k', 'ᵏ'),
    ('l', 'ˡ'),
    ('m', 'ᵐ'),
    ('n', 'ⁿ'),
    ('o', 'ᵒ'),
    ('p', 'ᵖ'),
    ('r', 'ʳ'),
    ('s', 'ˢ'),
    ('t', 'ᵗ'),
    ('u', 'ᵘ'),
    ('v', 'ᵛ'),
    ('w', 'ʷ'),
    ('x', 'ˣ'),
    ('y', 'ʸ'),
    ('z', 'ᶻ'),
];

/// Subscript mappings — fewer letters exist as true subscript glyphs, so
/// unmappable ones fall back to `_letter`.
const SUB: &[(char, char)] = &[
    ('0', '₀'),
    ('1', '₁'),
    ('2', '₂'),
    ('3', '₃'),
    ('4', '₄'),
    ('5', '₅'),
    ('6', '₆'),
    ('7', '₇'),
    ('8', '₈'),
    ('9', '₉'),
    ('+', '₊'),
    ('-', '₋'),
    ('=', '₌'),
    ('(', '₍'),
    (')', '₎'),
    ('a', 'ₐ'),
    ('e', 'ₑ'),
    ('h', 'ₕ'),
    ('i', 'ᵢ'),
    ('j', 'ⱼ'),
    ('k', 'ₖ'),
    ('l', 'ₗ'),
    ('m', 'ₘ'),
    ('n', 'ₙ'),
    ('o', 'ₒ'),
    ('p', 'ₚ'),
    ('r', 'ᵣ'),
    ('s', 'ₛ'),
    ('t', 'ₜ'),
    ('u', 'ᵤ'),
    ('v', 'ᵥ'),
    ('x', 'ₓ'),
];

/// Greek letters and common math operators → their Unicode glyphs.
fn symbols() -> &'static HashMap<&'static str, &'static str> {
    use std::sync::OnceLock;
    static SYMBOLS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    SYMBOLS.get_or_init(|| {
        [
            // Greek lowercase
            ("alpha", "α"),
            ("beta", "β"),
            ("gamma", "γ"),
            ("delta", "δ"),
            ("epsilon", "ε"),
            ("zeta", "ζ"),
            ("eta", "η"),
            ("theta", "θ"),
            ("iota", "ι"),
            ("kappa", "κ"),
            ("lambda", "λ"),
            ("mu", "μ"),
            ("nu", "ν"),
            ("xi", "ξ"),
            ("omicron", "ο"),
            ("pi", "π"),
            ("rho", "ρ"),
            ("sigma", "σ"),
            ("tau", "τ"),
            ("upsilon", "υ"),
            ("phi", "φ"),
            ("chi", "χ"),
            ("psi", "ψ"),
            ("omega", "ω"),
            // Greek uppercase
            ("Gamma", "Γ"),
            ("Delta", "Δ"),
            ("Theta", "Θ"),
            ("Lambda", "Λ"),
            ("Xi", "Ξ"),
            ("Pi", "Π"),
            ("Sigma", "Σ"),
            ("Phi", "Φ"),
            ("Psi", "Ψ"),
            ("Omega", "Ω"),
            // Large operators / accents-ish
            ("int", "∫"),
            ("oint", "∮"),
            ("sum", "∑"),
            ("prod", "∏"),
            ("infty", "∞"),
            ("partial", "∂"),
            ("nabla", "∇"),
            ("prime", "′"),
            ("degree", "°"),
            // Binary relations / operators
            ("cdot", "·"),
            ("times", "×"),
            ("div", "÷"),
            ("pm", "±"),
            ("mp", "∓"),
            ("approx", "≈"),
            ("equiv", "≡"),
            ("leq", "≤"),
            ("le", "≤"),
            ("geq", "≥"),
            ("ge", "≥"),
            ("neq", "≠"),
            ("ne", "≠"),
            ("in", "∈"),
            ("notin", "∉"),
            ("subset", "⊂"),
            ("subseteq", "⊆"),
            ("supset", "⊃"),
            ("cup", "∪"),
            ("cap", "∩"),
            ("land", "∧"),
            ("lor", "∨"),
            ("neg", "¬"),
            ("forall", "∀"),
            ("exists", "∃"),
            ("to", "→"),
            ("rightarrow", "→"),
            ("leftarrow", "←"),
            ("leftrightarrow", "↔"),
            ("Rightarrow", "⇒"),
            ("Leftarrow", "⇐"),
            ("dots", "…"),
            ("ldots", "…"),
            ("cdots", "⋯"),
        ]
        .into_iter()
        .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squares_become_superscript_digits() {
        assert_eq!(latex_to_unicode("E = mc^2"), "E = mc²");
        assert_eq!(latex_to_unicode("a^2 + b^2 = c^2"), "a² + b² = c²");
    }

    #[test]
    fn subscripts_become_unicode_digits() {
        assert_eq!(latex_to_unicode("x_1 + x_2"), "x₁ + x₂");
    }

    #[test]
    fn frac_and_sqrt_render_inline() {
        assert_eq!(latex_to_unicode("\\frac{\\sqrt{\\pi}}{2}"), "√π/2");
        assert_eq!(latex_to_unicode("\\frac{1}{2}"), "1/2");
        assert_eq!(latex_to_unicode("\\sqrt{x}"), "√x");
    }

    #[test]
    fn greek_and_operators_become_glyphs() {
        assert_eq!(latex_to_unicode("\\pi"), "π");
        assert_eq!(latex_to_unicode("\\infty"), "∞");
        assert_eq!(latex_to_unicode("\\alpha \\times \\beta"), "α × β");
        assert_eq!(latex_to_unicode("\\int_0^\\infty"), "∫₀^∞");
    }

    #[test]
    fn nested_exponent_renders_script_pass_through() {
        // e^{-x^2}: -x maps to ⁻ˣ, the already-superscript ² passes through.
        assert_eq!(latex_to_unicode("e^{-x^2}"), "e⁻ˣ²");
    }

    #[test]
    fn the_demo_formula_reads_as_unicode() {
        let body = "\\int_0^\\infty e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}";
        assert_eq!(latex_to_unicode(body), "∫₀^∞ e⁻ˣ² dx = √π/2");
    }

    #[test]
    fn unknown_macros_are_dropped_but_their_args_render() {
        assert_eq!(latex_to_unicode("\\bogus{x} + y"), "x + y");
        // Plain text with no markup passes through untouched.
        assert_eq!(latex_to_unicode("just some text"), "just some text");
    }

    #[test]
    fn grouping_braces_are_hidden() {
        assert_eq!(latex_to_unicode("{x}"), "x");
        // Multi-letter superscript maps each letter (b→ᵇ, c→ᶜ).
        assert_eq!(latex_to_unicode("a^{bc}"), "aᵇᶜ");
    }
}
