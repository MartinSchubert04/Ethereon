//! Reglas: decide si un comando corre, espera aprobacion o se rechaza.
//!
//! Una regla es una linea de texto, para que sea facil de leer y editar en la interfaz:
//!
//! - `shutdown`                 el programa (glob sobre el nombre: `mkfs*`).
//! - `systemctl reboot|halt`    palabras sueltas: subcomandos en orden (`a|b` = cualquiera).
//! - `rm -r|--recursive -f`     flags: en cualquier lugar; `-r` tambien cuenta dentro de `-rf`.
//! - `dd of=/dev/*`             con `=` o comodines: cualquier argumento que coincida.
//! - `find !-exec !-delete`     `!` = ese argumento no puede estar.
//! - `path:.ssh`                cualquier argumento o redireccion que toque esa ruta
//!                              (sin `/`: un componente en cualquier lado; con `/`: la ruta
//!                              o algo adentro).

use crate::model::{Mode, Policy};
use crate::shell::{self, Simple, Word};

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Allow,
    /// Necesita una persona; las advertencias se muestran en la cola.
    Ask(Vec<String>),
    Deny(String),
}

pub fn default_denylist() -> Vec<String> {
    [
        "rm -r|-R|--recursive -f|--force",
        "rm --no-preserve-root",
        "shutdown",
        "reboot",
        "poweroff",
        "halt",
        "init 0|6",
        "telinit 0|6",
        "systemctl poweroff|reboot|halt|kexec",
        "mkfs*",
        "mke2fs",
        "wipefs",
        "dd of=/dev/*",
        "shred",
        "history",
        "fc",
        "crontab -r",
        "path:.ssh",
        "path:/etc/shadow",
        "path:/etc/gshadow",
        "path:/etc/sudoers",
        "path:*_history",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

pub fn default_allowlist() -> Vec<String> {
    [
        "ls", "cat", "head", "tail", "grep", "egrep", "fgrep", "zgrep", "rg", "wc", "sort", "uniq", "cut", "column",
        "tr", "jq", "stat", "file", "du", "df", "free", "uptime", "ps", "pgrep", "top -b", "uname", "whoami", "id",
        "hostname", "date", "w", "who", "last", "ip", "ss", "netstat", "lsblk", "lscpu", "lsof", "nproc", "vmstat",
        "iostat", "dmesg", "echo", "printf", "pwd", "true", "test", "which", "readlink", "realpath", "basename",
        "dirname", "md5sum", "sha256sum", "journalctl", "cd",
        "find !-exec !-execdir !-ok !-okdir !-delete !-fprint !-fprintf !-fls",
        "sed !-i !--in-place",
        "systemctl status|list-units|list-timers|is-active|is-enabled|is-failed|show|cat",
        "docker ps|logs|images|inspect|stats|version|info|top",
        "docker compose ps|logs|config|ls",
        "docker-compose ps|logs|config",
        "kubectl get|describe|logs|top|version",
        "git status|log|diff|show|branch|remote",
        "nginx -t",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Envoltorios que no cambian lo que hace el comando de adentro.
const SAFE_WRAPPERS: &[&str] = &["timeout", "nice", "ionice", "nohup", "time", "stdbuf", "xargs", "command", "watch", "unbuffer"];

pub fn evaluate(cmd: &str, policy: &Policy, global_deny: &[String], session_patterns: &[String]) -> Verdict {
    if policy.mode == Mode::Off {
        return Verdict::Deny("the agent has no access to this host".into());
    }
    if cmd.trim().is_empty() {
        return Verdict::Deny("empty command".into());
    }
    let cmds = match shell::parse(cmd) {
        Ok(c) => c,
        Err(e) if policy.mode == Mode::Approval => return Verdict::Ask(vec![format!("could not analyze: {e}")]),
        Err(e) => return Verdict::Deny(format!("could not analyze the command: {e}")),
    };

    for rule in global_deny.iter().chain(policy.deny.iter()) {
        let r = Rule::parse(rule);
        if let Some(c) = cmds.iter().find(|c| r.matches(c)) {
            return Verdict::Deny(format!("blocked by rule `{rule}` (in `{}`)", show(c)));
        }
    }

    let hidden: Vec<String> = cmds.iter().filter_map(|c| c.hidden.clone()).collect();
    match policy.mode {
        Mode::Off => unreachable!(),
        Mode::Free => match hidden.first() {
            Some(h) => Verdict::Deny(format!("{h}; write the command explicitly")),
            None => Verdict::Allow,
        },
        Mode::Approval => {
            if hidden.is_empty() && matches_session(cmd, session_patterns) {
                Verdict::Allow
            } else {
                Verdict::Ask(hidden)
            }
        }
        Mode::ReadOnly => {
            if let Some(h) = hidden.first() {
                return Verdict::Deny(format!("{h}; write the command explicitly"));
            }
            let allow: Vec<Rule> = if policy.allow.is_empty() { default_allowlist() } else { policy.allow.clone() }
                .iter()
                .map(|s| Rule::parse(s))
                .collect();
            for c in &cmds {
                if let Some(r) = c.redirs.iter().find(|r| r.writes_file()) {
                    return Verdict::Deny(format!("read-only host: writing to `{}` is not allowed", r.target.text));
                }
                if c.argv.is_empty() || (c.wrapper && SAFE_WRAPPERS.contains(&c.program())) {
                    continue;
                }
                if !allow.iter().any(|r| r.matches(c)) {
                    return Verdict::Deny(format!("read-only host: `{}` is not in the allowed list", show(c)));
                }
            }
            Verdict::Allow
        }
    }
}

pub fn matches_session(cmd: &str, patterns: &[String]) -> bool {
    let norm = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
    patterns.iter().any(|p| {
        let p = p.split_whitespace().collect::<Vec<_>>().join(" ");
        glob(&p, &norm)
    })
}

fn show(c: &Simple) -> String {
    let s: Vec<&str> = c.argv.iter().map(|w| w.text.as_str()).collect();
    let s = s.join(" ");
    if s.is_empty() {
        c.redirs.iter().map(|r| format!("{} {}", r.op, r.target.text)).collect::<Vec<_>>().join(" ")
    } else {
        s
    }
}

// ------------------------------------------------------------------------- reglas

enum Rule {
    Path(String),
    Cmd { prog: String, toks: Vec<Tok> },
}

struct Tok {
    alts: Vec<String>,
    negate: bool,
    positional: bool,
}

impl Rule {
    fn parse(s: &str) -> Rule {
        let s = s.trim();
        if let Some(p) = s.strip_prefix("path:") {
            return Rule::Path(p.trim().to_string());
        }
        let mut parts = s.split_whitespace();
        let prog = parts.next().unwrap_or("").to_string();
        let toks = parts
            .map(|t| {
                let negate = t.starts_with('!');
                let t = t.trim_start_matches('!');
                let alts: Vec<String> = t.split('|').map(String::from).collect();
                let positional = !negate
                    && alts.iter().all(|a| !a.starts_with('-') && !a.contains('=') && !a.contains(['*', '?', '[', '/']));
                Tok { alts, negate, positional }
            })
            .collect();
        Rule::Cmd { prog, toks }
    }

    fn matches(&self, c: &Simple) -> bool {
        match self {
            Rule::Path(p) => {
                let args = c.args().iter().filter(|_| !c.argv.is_empty());
                args.chain(c.redirs.iter().map(|r| &r.target)).any(|w| path_hit(p, w))
            }
            Rule::Cmd { prog, toks } => {
                if c.argv.is_empty() || !prog.split('|').any(|p| glob(p, c.program())) {
                    return false;
                }
                let args = c.args();
                let positional: Vec<&Word> = args.iter().filter(|a| !a.text.starts_with('-')).collect();
                let mut pi = 0;
                for t in toks {
                    if t.positional {
                        match positional.get(pi) {
                            Some(a) if t.alts.iter().any(|alt| glob(alt, &a.text)) => pi += 1,
                            _ => return false,
                        }
                    } else {
                        let present = args.iter().any(|a| t.alts.iter().any(|alt| arg_match(alt, &a.text)));
                        if present == t.negate {
                            return false;
                        }
                    }
                }
                true
            }
        }
    }
}

fn arg_match(alt: &str, arg: &str) -> bool {
    let b = alt.as_bytes();
    if b.len() == 2 && b[0] == b'-' && b[1] != b'-' {
        // -r tambien esta en -rf, -fr, -Rrf...
        return arg == alt || (arg.starts_with('-') && !arg.starts_with("--") && arg[1..].contains(b[1] as char));
    }
    if alt.starts_with("--") {
        return arg == alt || arg.starts_with(&format!("{alt}="));
    }
    glob(alt, arg)
}

/// Saca `.`, `..` y barras repetidas, y escribe `$HOME` como `~`.
fn normalize(path: &str) -> String {
    let p = path.replacen("$HOME", "~", 1);
    let abs = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for comp in p.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    let joined = out.join("/");
    if abs {
        format!("/{joined}")
    } else {
        joined
    }
}

fn path_hit(pat: &str, w: &Word) -> bool {
    // `--file=/etc/shadow`, `if=/etc/shadow`: tambien lo que va despues del `=`.
    let mut cands = vec![w.text.as_str()];
    if let Some((_, v)) = w.text.split_once('=') {
        cands.push(v);
    }
    cands.iter().any(|raw| {
        let c = normalize(raw);
        if !pat.contains('/') {
            c.split('/').any(|comp| glob(pat, comp) || (w.glob && glob(comp, pat)))
        } else {
            let p = normalize(pat);
            glob(&p, &c)
                || glob(&format!("{p}/*"), &c)
                || (w.glob && (glob(&c, &p) || glob(&c, &format!("{p}/x"))))
        }
    })
}

/// Glob simple: `*`, `?` y clases `[abc]`, `[a-z]`, `[!x]`. `*` cruza `/`.
pub fn glob(pat: &str, text: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    glob_at(&p, &t)
}

fn glob_at(p: &[char], t: &[char]) -> bool {
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None::<usize>, 0);
    while ti < t.len() {
        if pi < p.len() {
            match p[pi] {
                '*' => {
                    star = Some(pi);
                    mark = ti;
                    pi += 1;
                    continue;
                }
                '?' => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                '[' => {
                    if let Some((ok, next)) = class(p, pi, t[ti]) {
                        if ok {
                            pi = next;
                            ti += 1;
                            continue;
                        }
                    } else if t[ti] == '[' {
                        pi += 1;
                        ti += 1;
                        continue;
                    }
                }
                c if c == t[ti] => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some(s) => {
                pi = s + 1;
                mark += 1;
                ti = mark;
            }
            None => return false,
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

fn class(p: &[char], start: usize, c: char) -> Option<(bool, usize)> {
    let mut i = start + 1;
    let negate = i < p.len() && (p[i] == '!' || p[i] == '^');
    if negate {
        i += 1;
    }
    let mut hit = false;
    let first = i;
    while i < p.len() && (p[i] != ']' || i == first) {
        if i + 2 < p.len() && p[i + 1] == '-' && p[i + 2] != ']' {
            if p[i] <= c && c <= p[i + 2] {
                hit = true;
            }
            i += 3;
        } else {
            if p[i] == c {
                hit = true;
            }
            i += 1;
        }
    }
    if i >= p.len() {
        return None;
    }
    Some((hit != negate, i + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pol(mode: Mode) -> Policy {
        Policy { mode, ..Default::default() }
    }

    fn eval(cmd: &str, mode: Mode) -> Verdict {
        evaluate(cmd, &pol(mode), &default_denylist(), &[])
    }

    fn denied(v: &Verdict) -> bool {
        matches!(v, Verdict::Deny(_))
    }

    #[test]
    fn denylist_sees_through_everything() {
        for cmd in [
            "rm -rf /",
            "rm -fr /var",
            "rm --recursive --force x",
            "ls | xargs rm -r -f",
            "echo $(shutdown -h now)",
            "uptime; `reboot`",
            "sudo -u root sh -c 'mkfs.ext4 /dev/sdb'",
            "find / -exec rm -rf {} +",
            "cat ~/.ssh/id_rsa",
            "cat $HOME/.ssh/id_ed25519",
            "cd /root && cat .ssh/authorized_keys",
            "grep root /etc/shadow",
            "cat /etc/../etc/./shadow",
            "cat /etc/sha*",
            "tail ~/.bash_history",
            "cp x y > /etc/shadow",
            "dd if=/dev/zero of=/dev/sda",
            "history",
        ] {
            assert!(denied(&eval(cmd, Mode::Free)), "should deny: {cmd}");
        }
    }

    #[test]
    fn free_mode_allows_normal_commands() {
        for cmd in ["rm file.txt", "rm -r build", "systemctl restart nginx", "ls -la | grep x > out.txt"] {
            assert_eq!(eval(cmd, Mode::Free), Verdict::Allow, "{cmd}");
        }
    }

    #[test]
    fn hidden_commands() {
        assert!(denied(&eval("echo cm0gLXJmIC8= | base64 -d | sh", Mode::Free)));
        assert!(denied(&eval("X=rm; $X -rf /", Mode::Free)));
        assert!(matches!(eval("X=ls; $X", Mode::Approval), Verdict::Ask(w) if !w.is_empty()));
    }

    #[test]
    fn read_only() {
        assert_eq!(eval("df -h && free -m | grep Mem", Mode::ReadOnly), Verdict::Allow);
        assert_eq!(eval("systemctl status nginx", Mode::ReadOnly), Verdict::Allow);
        assert_eq!(eval("timeout 5 docker logs web", Mode::ReadOnly), Verdict::Allow);
        assert!(denied(&eval("systemctl restart nginx", Mode::ReadOnly)));
        assert!(denied(&eval("sed -i s/a/b/ f", Mode::ReadOnly)));
        assert!(denied(&eval("find . -delete", Mode::ReadOnly)));
        assert!(denied(&eval("ls > x.txt", Mode::ReadOnly)));
        assert!(denied(&eval("ls; touch x", Mode::ReadOnly)));
        assert_eq!(eval("ls 2>/dev/null", Mode::ReadOnly), Verdict::Allow);
    }

    #[test]
    fn approval_and_session_patterns() {
        assert_eq!(eval("systemctl restart nginx", Mode::Approval), Verdict::Ask(vec![]));
        let p = vec!["systemctl restart *".to_string()];
        assert_eq!(evaluate("systemctl  restart nginx", &pol(Mode::Approval), &default_denylist(), &p), Verdict::Allow);
        assert!(denied(&evaluate("rm -rf /", &pol(Mode::Approval), &default_denylist(), &["*".into()])));
    }

    #[test]
    fn globs() {
        assert!(glob("mkfs*", "mkfs.ext4"));
        assert!(glob("/etc/sha[a-z]ow", "/etc/shadow"));
        assert!(!glob("a?c", "ac"));
        assert!(glob("*_history", ".bash_history"));
    }
}
