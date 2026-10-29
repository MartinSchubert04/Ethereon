//! Parser de comandos de shell (sintaxis POSIX/bash, lo suficiente para las reglas).
//!
//! Convierte una linea en la lista de comandos simples que se van a ejecutar: separa
//! por `|`, `;`, `&&`, `||`, `&` y saltos de linea, y entra en `$( )`, backticks,
//! `<( )`, `sh -c '...'`, `eval`, `find -exec`, y envoltorios como `sudo`, `xargs`,
//! `timeout` o `env`. Cuando no puede saber que se va a correr (un nombre de programa
//! que sale de una variable, algo que entra a una shell por un pipe) lo marca como
//! `hidden` y las reglas lo tratan con desconfianza.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Word {
    /// Texto sin comillas. Las expansiones quedan como `$VAR` o `$(...)`.
    pub text: String,
    /// Tiene `$VAR`, `${}`, `$( )` o backticks: el valor real se sabe recien al correr.
    pub dynamic: bool,
    /// Tiene `*`, `?`, `[` o llaves sin comillas: la shell lo expande.
    pub glob: bool,
    pub quoted: bool,
    /// Algo adentro de la palabra hace que no se pueda saber que corre (por ejemplo un
    /// comando dentro de `${...}`); el motivo. El comando entero queda como oculto.
    pub hide: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Redir {
    pub op: String,
    pub target: Word,
}

impl Redir {
    /// Escribe en un archivo (no en /dev/null ni en otro descriptor).
    pub fn writes_file(&self) -> bool {
        self.op.contains('>') && self.target.text != "/dev/null"
    }
}

#[derive(Clone, Debug, Default)]
pub struct Simple {
    pub argv: Vec<Word>,
    pub redirs: Vec<Redir>,
    /// Recibe la salida de otro comando por un pipe.
    pub stdin_piped: bool,
    /// Es un envoltorio (`sudo`, `xargs`...): el comando de adentro aparece aparte.
    pub wrapper: bool,
    /// No se puede saber que va a correr; el motivo.
    pub hidden: Option<String>,
}

impl Simple {
    pub fn program(&self) -> &str {
        self.argv.first().map(|w| basename(&w.text)).unwrap_or("")
    }
    pub fn args(&self) -> &[Word] {
        if self.argv.is_empty() {
            &[]
        } else {
            &self.argv[1..]
        }
    }
}

pub fn basename(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

/// Caracteres que no se ven pero cambian lo que corre: control (menos tab/nl/cr), marcas
/// de direccion (bidi), de ancho cero y espacios que no son ASCII (la shell no los toma como
/// separador: `ls<U+00A0>-la` es un solo programa). Si aparece uno, el comando no es lo que parece.
pub fn suspicious_char(src: &str) -> Option<String> {
    for c in src.chars() {
        let bad = (c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
            || (c.is_whitespace() && !c.is_ascii())
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{00AD}');
        if bad {
            return Some(format!("the command has a hidden character (U+{:04X})", c as u32));
        }
    }
    None
}

pub fn parse(src: &str) -> Result<Vec<Simple>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    parse_into(&chars, &mut out, 0)?;
    Ok(out)
}

const MAX_DEPTH: usize = 8;

fn is_op(c: char) -> bool {
    matches!(c, ';' | '&' | '|' | '(' | ')' | '<' | '>' | '\n')
}

fn parse_into(s: &[char], out: &mut Vec<Simple>, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("command is nested too deeply".into());
    }
    let mut i = 0;
    let mut cur = Simple::default();
    let mut piped = false;
    let n = s.len();
    while i < n {
        let c = s[i];
        if c == ' ' || c == '\t' || c == '\r' {
            i += 1;
            continue;
        }
        if c == '\\' && i + 1 < n && s[i + 1] == '\n' {
            i += 2;
            continue;
        }
        match c {
            '\n' | ';' => {
                finish(&mut cur, &mut piped, false, out, depth)?;
                i += 1;
            }
            '&' if i + 1 < n && s[i + 1] == '>' => {
                // &> archivo  /  &>> archivo
                let op = if i + 2 < n && s[i + 2] == '>' { "&>>" } else { "&>" };
                i += op.len();
                let (w, ni) = redirect_target(s, i, out, depth)?;
                i = ni;
                cur.redirs.push(Redir { op: op.into(), target: w });
            }
            '&' => {
                i += if i + 1 < n && s[i + 1] == '&' { 2 } else { 1 };
                finish(&mut cur, &mut piped, false, out, depth)?;
            }
            '|' => {
                if i + 1 < n && s[i + 1] == '|' {
                    i += 2;
                    finish(&mut cur, &mut piped, false, out, depth)?;
                } else {
                    i += if i + 1 < n && s[i + 1] == '&' { 2 } else { 1 };
                    finish(&mut cur, &mut piped, true, out, depth)?;
                }
            }
            '(' | ')' | '{' | '}' if c != '{' && c != '}' || is_lone_brace(s, i) => {
                finish(&mut cur, &mut piped, false, out, depth)?;
                i += 1;
            }
            '#' => {
                while i < n && s[i] != '\n' {
                    i += 1;
                }
            }
            '<' | '>' if i + 1 < n && s[i + 1] == '(' => {
                // Sustitucion de proceso <( ) / >( ): se analiza adentro.
                let end = matching_paren(s, i + 2)?;
                parse_into(&s[i + 2..end], out, depth + 1)?;
                cur.argv.push(Word { text: "<(...)".into(), dynamic: true, ..Default::default() });
                i = end + 1;
            }
            '<' | '>' => {
                let (op, ni) = read_redirect_op(s, i, String::new());
                i = ni;
                if op == "<<" || op == "<<-" {
                    return Err("heredocs are not supported; use a file or printf".into());
                }
                let (w, ni) = redirect_target(s, i, out, depth)?;
                i = ni;
                // >&2, <&0: duplicar un descriptor, no es un archivo.
                let dup = op.ends_with('&') && (w.text == "-" || w.text.chars().all(|c| c.is_ascii_digit()));
                if !dup {
                    cur.redirs.push(Redir { op, target: w });
                }
            }
            d if d.is_ascii_digit() && starts_fd_redirect(s, i) => {
                let mut fd = String::new();
                while s[i].is_ascii_digit() {
                    fd.push(s[i]);
                    i += 1;
                }
                let (op, ni) = read_redirect_op(s, i, fd);
                i = ni;
                let (w, ni) = redirect_target(s, i, out, depth)?;
                i = ni;
                let dup = op.ends_with('&') && (w.text == "-" || w.text.chars().all(|c| c.is_ascii_digit()));
                if !dup {
                    cur.redirs.push(Redir { op, target: w });
                }
            }
            _ => {
                let (w, ni) = read_word(s, i, out, depth)?;
                i = ni;
                cur.argv.push(w);
            }
        }
    }
    finish(&mut cur, &mut piped, false, out, depth)
}

/// `{` y `}` solo separan cuando van sueltos (grupos `{ cmd; }`), no en `a{b,c}`.
fn is_lone_brace(s: &[char], i: usize) -> bool {
    let before = i == 0 || s[i - 1].is_whitespace() || matches!(s[i - 1], ';' | '&' | '|');
    let after = i + 1 >= s.len() || s[i + 1].is_whitespace() || matches!(s[i + 1], ';');
    before && after
}

fn starts_fd_redirect(s: &[char], mut i: usize) -> bool {
    // Solo al principio de una palabra: "2>err" si, "a2>b" no llega aca.
    if i > 0 && !s[i - 1].is_whitespace() && !is_op(s[i - 1]) {
        return false;
    }
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
    }
    i < s.len() && (s[i] == '<' || s[i] == '>')
}

fn read_redirect_op(s: &[char], mut i: usize, mut op: String) -> (String, usize) {
    let first = s[i];
    op.push(first);
    i += 1;
    while i < s.len() && op.len() < 4 {
        let c = s[i];
        let ok = match first {
            '>' => matches!(c, '>' | '|' | '&') && !op.ends_with('&'),
            _ => matches!(c, '<' | '>' | '&' | '-') && !op.ends_with('&') && !op.ends_with('-'),
        };
        if !ok {
            break;
        }
        op.push(c);
        i += 1;
    }
    (op, i)
}

fn redirect_target(s: &[char], mut i: usize, out: &mut Vec<Simple>, depth: usize) -> Result<(Word, usize), String> {
    while i < s.len() && (s[i] == ' ' || s[i] == '\t') {
        i += 1;
    }
    if i >= s.len() || is_op(s[i]) {
        return Err("redirection without a target".into());
    }
    read_word(s, i, out, depth)
}

/// Indice del `)` que cierra, empezando adentro del parentesis.
fn matching_paren(s: &[char], mut i: usize) -> Result<usize, String> {
    let mut depth = 1;
    while i < s.len() {
        match s[i] {
            '\\' => i += 1,
            '\'' => {
                i += 1;
                while i < s.len() && s[i] != '\'' {
                    i += 1;
                }
            }
            '"' => {
                i += 1;
                while i < s.len() && s[i] != '"' {
                    if s[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err("unbalanced parenthesis".into())
}

/// Indice de la `}` que cierra un `${` cuyo contenido empieza en `i`.
fn matching_brace(s: &[char], mut i: usize) -> Result<usize, String> {
    let mut depth = 1;
    while i < s.len() {
        match s[i] {
            '\\' => i += 1,
            '\'' => {
                i += 1;
                while i < s.len() && s[i] != '\'' {
                    i += 1;
                }
            }
            '`' => {
                let (_, ni) = read_backtick(s, i + 1)?;
                i = ni;
                continue;
            }
            '$' if i + 1 < s.len() && s[i + 1] == '(' => {
                i = matching_paren(s, i + 2)?;
            }
            '$' if i + 1 < s.len() && s[i + 1] == '{' => {
                depth += 1;
                i += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err("unterminated ${".into())
}

/// Busca comandos (`$( )`, backticks, `${}` anidados) dentro del cuerpo de un `${...}`.
fn scan_expansion(body: &[char], out: &mut Vec<Simple>, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("command is nested too deeply".into());
    }
    let mut k = 0;
    let mut scratch = Word::default();
    while k < body.len() {
        match body[k] {
            '\\' => k += 2,
            '\'' => {
                k += 1;
                while k < body.len() && body[k] != '\'' {
                    k += 1;
                }
                k += 1;
            }
            '$' => k = read_dollar(body, k, &mut scratch, out, depth)?,
            '`' => {
                let (inner, ni) = read_backtick(body, k + 1)?;
                let chars: Vec<char> = inner.chars().collect();
                parse_into(&chars, out, depth + 1)?;
                k = ni;
            }
            _ => k += 1,
        }
    }
    Ok(())
}

fn read_backtick(s: &[char], mut i: usize) -> Result<(String, usize), String> {
    let mut inner = String::new();
    while i < s.len() {
        match s[i] {
            '\\' if i + 1 < s.len() => {
                inner.push(s[i + 1]);
                i += 2;
            }
            '`' => return Ok((inner, i + 1)),
            c => {
                inner.push(c);
                i += 1;
            }
        }
    }
    Err("unterminated backtick".into())
}

/// `$...` a partir de `i` (que apunta al `$`). Devuelve el indice siguiente.
fn read_dollar(s: &[char], i: usize, w: &mut Word, out: &mut Vec<Simple>, depth: usize) -> Result<usize, String> {
    let n = s.len();
    if i + 1 >= n {
        w.text.push('$');
        return Ok(i + 1);
    }
    match s[i + 1] {
        '(' => {
            let end = matching_paren(s, i + 2)?;
            parse_into(&s[i + 2..end], out, depth + 1)?;
            w.text.push_str("$(...)");
            w.dynamic = true;
            Ok(end + 1)
        }
        '{' => {
            // La llave que cierra de verdad: puede haber `${` anidados, comillas, `$( )` y
            // backticks adentro (`${x:-${y:-$(cmd)}}`).
            let j = matching_brace(s, i + 2)?;
            let body = &s[i + 2..j];
            // Los comandos de adentro corren: se analizan como cualquier otro, y ademas el
            // comando entero queda marcado como oculto.
            let before = out.len();
            scan_expansion(body, out, depth + 1)?;
            if out.len() > before {
                w.hide = Some("a command inside `${...}` runs at run time".into());
            }
            // `${x@P}` expande el valor como un prompt, y eso puede ejecutar comandos.
            if body.ends_with(&['@', 'P']) {
                w.hide = Some("`${...@P}` can run commands stored in a variable".into());
            }
            let name: String = body.iter().collect();
            w.text.push('$');
            w.text.push_str(&name);
            w.dynamic = true;
            Ok(j + 1)
        }
        c if c.is_ascii_alphabetic() || c == '_' => {
            let mut j = i + 1;
            w.text.push('$');
            while j < n && (s[j].is_ascii_alphanumeric() || s[j] == '_') {
                w.text.push(s[j]);
                j += 1;
            }
            w.dynamic = true;
            Ok(j)
        }
        c if c.is_ascii_digit() || "@*#?$!-".contains(c) => {
            w.text.push('$');
            w.text.push(c);
            w.dynamic = true;
            Ok(i + 2)
        }
        '\'' => {
            // $'...' (ANSI-C): puede esconder cualquier caracter con \x..
            let mut j = i + 2;
            while j < n && s[j] != '\'' {
                if s[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            w.text.push_str("$'...'");
            w.dynamic = true;
            Ok(j + 1)
        }
        _ => {
            w.text.push('$');
            Ok(i + 1)
        }
    }
}

fn read_word(s: &[char], mut i: usize, out: &mut Vec<Simple>, depth: usize) -> Result<(Word, usize), String> {
    let mut w = Word::default();
    let n = s.len();
    while i < n {
        let c = s[i];
        // Los mismos separadores que el bucle de `parse_into` (los de bash); si no, un espacio
        // Unicode daria una palabra vacia sin avanzar y el parser no terminaria nunca.
        if matches!(c, ' ' | '\t' | '\r' | '\n') || is_op(c) {
            break;
        }
        match c {
            '\\' => {
                if i + 1 < n {
                    if s[i + 1] != '\n' {
                        w.text.push(s[i + 1]);
                        w.quoted = true;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '\'' => {
                w.quoted = true;
                let start = i + 1;
                let mut j = start;
                while j < n && s[j] != '\'' {
                    j += 1;
                }
                if j >= n {
                    return Err("unterminated single quote".into());
                }
                w.text.extend(&s[start..j]);
                i = j + 1;
            }
            '"' => {
                w.quoted = true;
                i += 1;
                loop {
                    if i >= n {
                        return Err("unterminated double quote".into());
                    }
                    match s[i] {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' if i + 1 < n && "$`\"\\\n".contains(s[i + 1]) => {
                            if s[i + 1] != '\n' {
                                w.text.push(s[i + 1]);
                            }
                            i += 2;
                        }
                        '$' => i = read_dollar(s, i, &mut w, out, depth)?,
                        '`' => {
                            let (inner, ni) = read_backtick(s, i + 1)?;
                            let chars: Vec<char> = inner.chars().collect();
                            parse_into(&chars, out, depth + 1)?;
                            w.text.push_str("`...`");
                            w.dynamic = true;
                            i = ni;
                        }
                        ch => {
                            w.text.push(ch);
                            i += 1;
                        }
                    }
                }
            }
            '$' => i = read_dollar(s, i, &mut w, out, depth)?,
            '`' => {
                let (inner, ni) = read_backtick(s, i + 1)?;
                let chars: Vec<char> = inner.chars().collect();
                parse_into(&chars, out, depth + 1)?;
                w.text.push_str("`...`");
                w.dynamic = true;
                i = ni;
            }
            '*' | '?' | '[' => {
                w.glob = true;
                w.text.push(c);
                i += 1;
            }
            '{' => {
                // Expansion de llaves: {rm,-rf,/} arma un comando entero.
                let rest: String = s[i..].iter().take_while(|c| !c.is_whitespace()).collect();
                if rest.contains(',') || rest.contains("..") {
                    w.glob = true;
                }
                w.text.push(c);
                i += 1;
            }
            _ => {
                w.text.push(c);
                i += 1;
            }
        }
    }
    Ok((w, i))
}

const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "while", "until", "do", "done", "case", "esac", "!", "{", "}", "function",
    "select", "coproc", "time",
];
const SHELLS: &[&str] = &["sh", "bash", "zsh", "dash", "ksh", "ash", "fish", "mksh", "csh", "tcsh"];

fn is_assignment(w: &Word) -> bool {
    match w.text.find('=') {
        Some(p) if p > 0 => {
            let name = &w.text[..p];
            let mut ch = name.chars();
            let first = ch.next().unwrap();
            (first.is_ascii_alphabetic() || first == '_') && ch.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

fn finish(cur: &mut Simple, piped: &mut bool, next_piped: bool, out: &mut Vec<Simple>, depth: usize) -> Result<(), String> {
    let mut simple = std::mem::take(cur);
    simple.stdin_piped = *piped;
    *piped = next_piped;
    if simple.argv.is_empty() && simple.redirs.is_empty() {
        return Ok(());
    }
    expand(simple, out, depth)
}

/// Saca palabras clave y asignaciones del principio, y abre envoltorios.
fn expand(mut simple: Simple, out: &mut Vec<Simple>, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("command is nested too deeply".into());
    }
    // Una palabra con algo oculto adentro (tambien en asignaciones o redirecciones) vuelve
    // oculto el comando entero.
    let hide = simple.argv.iter().chain(simple.redirs.iter().map(|r| &r.target)).find_map(|w| w.hide.clone());
    if let Some(h) = hide {
        simple.hidden.get_or_insert(h);
        if simple.argv.iter().all(is_assignment) {
            // Solo asignaciones: igual se registra, para que las reglas lo vean.
            out.push(simple);
            return Ok(());
        }
    }
    while let Some(w) = simple.argv.first() {
        if (!w.quoted && KEYWORDS.contains(&w.text.as_str())) || is_assignment(w) {
            simple.argv.remove(0);
        } else {
            break;
        }
    }
    if simple.argv.is_empty() {
        if !simple.redirs.is_empty() {
            out.push(simple);
        }
        return Ok(());
    }
    let head = &simple.argv[0];
    if head.dynamic || head.glob {
        simple.hidden = Some(format!("the program name `{}` is only known at run time", head.text));
        out.push(simple);
        return Ok(());
    }
    let prog = simple.program().to_string();
    let args: Vec<Word> = simple.args().to_vec();
    let stdin_piped = simple.stdin_piped;

    // Envoltorios: el comando de adentro tambien se revisa.
    if let Some(skip) = wrapper_skip(&prog, &args) {
        let inner_start = skip.min(args.len());
        let mut inner = Simple { argv: args[inner_start..].to_vec(), stdin_piped, ..Default::default() };
        if prog == "env" && args.iter().any(|a| a.text == "-S" || a.text.starts_with("--split-string")) {
            simple.hidden = Some("`env -S` builds the command from a string".into());
        }
        simple.wrapper = true;
        out.push(simple);
        if prog == "xargs" && inner.argv.is_empty() {
            inner.argv.push(Word { text: "echo".into(), ..Default::default() });
        }
        if !inner.argv.is_empty() {
            expand(inner, out, depth + 1)?;
        }
        return Ok(());
    }

    // `su`/`runuser`/`script` corren un string con `-c`, igual que `sh -c`.
    if matches!(prog.as_str(), "su" | "runuser" | "script") {
        // runuser/su ... -- programa args  (corre ese programa)
        if let Some(pos) = args.iter().position(|a| a.text == "--") {
            let inner = Simple { argv: args[pos + 1..].to_vec(), ..Default::default() };
            simple.wrapper = true;
            out.push(simple);
            if !inner.argv.is_empty() {
                expand(inner, out, depth + 1)?;
            }
            return Ok(());
        }
        let mut it = args.iter();
        let mut script: Option<&Word> = None;
        while let Some(a) = it.next() {
            if a.text == "-c" || a.text == "--command" {
                script = it.next();
                break;
            }
            if let Some(v) = a.text.strip_prefix("--command=").or_else(|| a.text.strip_prefix("-c")).filter(|_| a.text.starts_with("-c") && a.text.len() > 2) {
                let chars: Vec<char> = v.chars().collect();
                parse_into(&chars, out, depth + 1)?;
                out.push(simple);
                return Ok(());
            }
        }
        match script {
            Some(w) if w.dynamic => simple.hidden = Some("the command comes from a variable".into()),
            Some(w) => {
                let chars: Vec<char> = w.text.chars().collect();
                parse_into(&chars, out, depth + 1)?;
            }
            None => {}
        }
        out.push(simple);
        return Ok(());
    }

    // Interpretes que corren codigo venido en un argumento: no se puede ver que hacen.
    const INLINE_CODE: &[(&str, &[&str])] = &[
        ("perl", &["-e", "-E"]),
        ("python", &["-c"]),
        ("python2", &["-c"]),
        ("python3", &["-c"]),
        ("ruby", &["-e"]),
        ("node", &["-e", "--eval", "-p", "--print"]),
        ("nodejs", &["-e", "--eval", "-p", "--print"]),
        ("php", &["-r"]),
        ("lua", &["-e"]),
        ("tclsh", &["-"]),
        ("gdb", &["-ex"]),
    ];
    if let Some((_, flags)) = INLINE_CODE.iter().find(|(name, _)| *name == prog) {
        if args.iter().any(|a| flags.iter().any(|f| a.text == *f || (f.len() == 2 && a.text.starts_with(f)))) {
            simple.hidden = Some(format!("`{prog}` runs code given in an argument"));
        }
        out.push(simple);
        return Ok(());
    }
    // awk/sed: un `system()` o un comando `e` corre una orden de shell.
    if matches!(prog.as_str(), "awk" | "gawk" | "mawk" | "nawk") {
        if args.iter().any(|a| !a.text.starts_with('-') && (a.text.contains("system(") || a.text.contains("|getline") || a.text.contains("| getline") || a.text.contains("print |") || a.text.contains("printf |"))) {
            simple.hidden = Some("this awk program runs a shell command".into());
        }
        out.push(simple);
        return Ok(());
    }
    if prog == "sed" {
        // Comandos `e` (ejecuta) o `w`/`W`/`r`/`R` (escriben/leen archivos) en el script.
        if args.iter().any(|a| {
            let t = a.text.trim_start_matches('-');
            (a.text.starts_with("-e") || a.text.starts_with("--expression") || !a.text.starts_with('-'))
                && (t.contains(" e ") || t.ends_with("e") && t.len() <= 2 || t.contains("1e") || t.contains(";e") || t.contains("}e"))
        }) {
            simple.hidden = Some("this sed script can run a command (`e`)".into());
        }
        out.push(simple);
        return Ok(());
    }
    // Ejecutores cuyas opciones son dificiles de separar, o que corren en OTRO host (ssh):
    // no se intenta analizar, se marca oculto.
    // kubectl/oc exec|rsh|debug|attach corren comandos en un pod (otro sistema): como ssh.
    if matches!(prog.as_str(), "kubectl" | "oc" | "kubectl.exe") {
        if args.iter().any(|a| matches!(a.text.as_str(), "exec" | "rsh" | "debug" | "attach" | "port-forward" | "cp" | "proxy")) {
            simple.hidden = Some(format!("`{prog}` runs a command on a pod that cannot be checked here"));
            out.push(simple);
            return Ok(());
        }
    }
    // `ip netns exec NS programa ...` corre un comando dentro de un namespace.
    if prog == "ip" {
        if let Some(i) = args.iter().position(|a| a.text == "exec") {
            if args[..i].iter().any(|a| a.text == "netns") {
                let inner = Simple { argv: args.get(i + 2..).map(|x| x.to_vec()).unwrap_or_default(), ..Default::default() };
                simple.wrapper = true;
                out.push(simple);
                if !inner.argv.is_empty() {
                    expand(inner, out, depth + 1)?;
                }
                return Ok(());
            }
        }
    }
    const OPAQUE: &[&str] = &["systemd-run", "unshare", "nsenter", "setpriv", "capsh", "ssh", "scp", "sftp", "sshpass", "rsync", "at", "batch"];
    if OPAQUE.contains(&prog.as_str()) {
        simple.hidden = Some(format!("`{prog}` can run a command that cannot be checked here"));
        out.push(simple);
        return Ok(());
    }

    if SHELLS.contains(&prog.as_str()) || prog == "busybox" && args.first().is_some_and(|a| SHELLS.contains(&a.text.as_str())) {
        let mut script: Option<&Word> = None;
        let mut positional = false;
        let mut it = args.iter().peekable();
        while let Some(a) = it.next() {
            if a.text.starts_with('-') && !a.text.starts_with("--") && a.text[1..].contains('c') {
                script = it.next();
                break;
            }
            if !a.text.starts_with('-') && !SHELLS.contains(&a.text.as_str()) {
                positional = true;
                break;
            }
        }
        match script {
            Some(w) if w.dynamic => simple.hidden = Some("the shell script comes from a variable".into()),
            Some(w) => {
                let chars: Vec<char> = w.text.chars().collect();
                parse_into(&chars, out, depth + 1)?;
            }
            None if !positional => {
                let fed = stdin_piped || simple.redirs.iter().any(|r| r.op.starts_with('<'));
                if fed {
                    simple.hidden = Some("piping or redirecting into a shell hides the real command".into());
                }
            }
            None => {}
        }
        out.push(simple);
        return Ok(());
    }

    match prog.as_str() {
        "eval" => {
            if args.iter().any(|a| a.dynamic) {
                simple.hidden = Some("`eval` of a variable".into());
            } else {
                let joined: Vec<String> = args.iter().map(|a| a.text.clone()).collect();
                let chars: Vec<char> = joined.join(" ").chars().collect();
                parse_into(&chars, out, depth + 1)?;
            }
            out.push(simple);
        }
        "find" => {
            let mut k = 0;
            while k < args.len() {
                if matches!(args[k].text.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir") {
                    let start = k + 1;
                    let mut end = start;
                    while end < args.len() && args[end].text != ";" && args[end].text != "+" {
                        end += 1;
                    }
                    let inner = Simple { argv: args[start..end].to_vec(), ..Default::default() };
                    if !inner.argv.is_empty() {
                        expand(inner, out, depth + 1)?;
                    }
                    k = end;
                }
                k += 1;
            }
            out.push(simple);
        }
        _ => out.push(simple),
    }
    Ok(())
}

/// Si `prog` es un envoltorio, cuantos argumentos saltear hasta el comando de adentro.
fn wrapper_skip(prog: &str, args: &[Word]) -> Option<usize> {
    // Opciones que llevan un valor aparte.
    let with_value: &[&str] = match prog {
        "sudo" => &["-u", "-g", "-h", "-p", "-C", "-U", "-r", "-t", "-T", "-D", "--user", "--group", "--prompt"],
        "doas" => &["-u", "-C"],
        "env" => &["-u", "-C", "--unset", "--chdir"],
        "nice" => &["-n", "--adjustment"],
        "ionice" => &["-c", "-n", "-p", "-P", "-u"],
        "timeout" => &["-s", "-k", "--signal", "--kill-after"],
        "stdbuf" => &["-i", "-o", "-e"],
        "exec" => &["-a"],
        "xargs" => &["-I", "-n", "-P", "-L", "-d", "-E", "-s", "-a", "--max-args", "--max-procs", "--delimiter"],
        "watch" => &["-n", "--interval", "-d"],
        "strace" | "ltrace" => &["-e", "-o", "-p", "-s", "-u"],
        "nohup" | "command" | "builtin" | "unbuffer" | "setsid" | "time" | "chrt" | "taskset" | "flock" | "chroot"
        | "busybox" => &[],
        _ => return None,
    };
    let mut i = 0;
    // timeout DURACION cmd / chroot DIR cmd / flock ARCHIVO cmd / taskset MASK cmd / chrt PRIO cmd
    let mut positional_before = matches!(prog, "timeout" | "chroot" | "flock" | "taskset" | "chrt") as usize;
    while i < args.len() {
        let t = &args[i].text;
        if t == "--" {
            i += 1;
            break;
        }
        if t.starts_with('-') && t.len() > 1 {
            if with_value.contains(&t.as_str()) {
                i += 1;
            }
            i += 1;
            continue;
        }
        if prog == "env" && is_assignment(&args[i]) {
            i += 1;
            continue;
        }
        if positional_before > 0 {
            positional_before -= 1;
            i += 1;
            continue;
        }
        break;
    }
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progs(s: &str) -> Vec<String> {
        parse(s).unwrap().iter().map(|c| c.program().to_string()).collect()
    }

    #[test]
    fn splits_operators() {
        assert_eq!(progs("ls -la | grep x && echo ok; df -h || true & uptime"), ["ls", "grep", "echo", "df", "true", "uptime"]);
    }

    #[test]
    fn enters_substitutions() {
        let p = progs("echo $(rm -rf /tmp/x) `shutdown now` \"$(reboot)\"");
        assert!(p.contains(&"rm".to_string()));
        assert!(p.contains(&"shutdown".to_string()));
        assert!(p.contains(&"reboot".to_string()));
        assert!(progs("diff <(cat a) <(mkfs /dev/sda)").contains(&"mkfs".to_string()));
    }

    #[test]
    fn enters_wrappers_and_shells() {
        let p = progs("sudo -u root timeout 5 sh -c 'cd /; rm -rf x'");
        assert!(p.contains(&"rm".to_string()) && p.contains(&"cd".to_string()));
        assert!(progs("find . -name '*.log' -exec rm -f {} \\;").contains(&"rm".to_string()));
        assert!(progs("ls | xargs -n1 shred").contains(&"shred".to_string()));
        assert!(progs("FOO=1 env BAR=2 nohup reboot").contains(&"reboot".to_string()));
        assert!(progs("eval 'shutdown -h now'").contains(&"shutdown".to_string()));
    }

    #[test]
    fn quotes_hide_nothing() {
        let c = parse("'r''m' \"-rf\" /").unwrap();
        assert_eq!(c[0].program(), "rm");
        assert_eq!(c[0].args()[0].text, "-rf");
        assert_eq!(parse("r\\m -rf /").unwrap()[0].program(), "rm");
    }

    #[test]
    fn marks_hidden() {
        assert!(parse("$CMD -rf /").unwrap()[0].hidden.is_some());
        assert!(parse("{rm,-rf,/}").unwrap()[0].hidden.is_some());
        assert!(parse("echo cm0= | base64 -d | bash").unwrap()[2].hidden.is_some());
        assert!(parse("sh < script.sh").unwrap()[0].hidden.is_some());
        assert!(parse("bash script.sh").unwrap()[0].hidden.is_none());
    }

    #[test]
    fn redirections() {
        let c = parse("cat a 2>/dev/null > out.txt 2>&1").unwrap();
        assert_eq!(c[0].redirs.len(), 2);
        assert!(!c[0].redirs[0].writes_file());
        assert!(c[0].redirs[1].writes_file());
        assert!(parse("cat <<EOF\nx\nEOF").is_err());
    }

    #[test]
    fn errors_on_unbalanced() {
        assert!(parse("echo 'abc").is_err());
        assert!(parse("echo $(ls").is_err());
    }

    /// Un espacio Unicode colgaba el parser (palabra vacia sin avanzar). Ahora es parte de la
    /// palabra, como en bash.
    #[test]
    fn unicode_space_is_part_of_the_word() {
        let c = parse("ls\u{00a0}-la \u{2003}x").unwrap();
        assert_eq!(c.len(), 1);
        let argv: Vec<&str> = c[0].argv.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(argv, ["ls\u{00a0}-la", "\u{2003}x"]);
    }
}
