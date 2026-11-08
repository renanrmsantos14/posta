//! Pastas padrão de cada conta: qual nome do servidor faz o papel de Enviados, Arquivo, Lixeira e Spam.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Inbox,
    Sent,
    Archive,
    Trash,
    Junk,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Inbox, Role::Sent, Role::Archive, Role::Trash, Role::Junk];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Inbox => "inbox",
            Role::Sent => "sent",
            Role::Archive => "archive",
            Role::Trash => "trash",
            Role::Junk => "junk",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub role: Role,
    pub name: String,
}

/// Uma linha do LIST, já reduzida ao que importa aqui.
#[derive(Debug, Clone)]
pub struct Listed {
    pub name: String,
    /// Atributo SPECIAL-USE: "sent", "archive", "all", "trash" ou "junk".
    pub special: Option<&'static str>,
}

/// Nomes comuns quando o servidor não marca a pasta (comparação sem maiúsculas; também sob "INBOX.").
const NAMES: [(Role, &[&str]); 4] = [
    (Role::Sent, &["Sent", "Sent Items", "Sent Messages", "Enviados", "Itens Enviados"]),
    (Role::Archive, &["Archive", "Archives", "Arquivo", "Arquivados"]),
    (Role::Trash, &["Trash", "Deleted Items", "Deleted Messages", "Lixeira", "Itens Excluídos"]),
    (Role::Junk, &["Junk", "Junk E-mail", "Spam", "Lixo Eletrônico"]),
];

fn by_name<'a>(listed: &'a [Listed], wanted: &[&str]) -> Option<&'a Listed> {
    wanted.iter().find_map(|w| {
        listed.iter().find(|l| {
            let n = l.name.strip_prefix("INBOX.").or_else(|| l.name.strip_prefix("INBOX/")).unwrap_or(&l.name);
            n.to_lowercase() == w.to_lowercase()
        })
    })
}

/// Escolhe a pasta de cada papel. SPECIAL-USE ganha de nome; para Arquivo, \Archive ganha de \All (Gmail).
pub fn pick(listed: &[Listed]) -> Vec<Folder> {
    let special = |tag: &str| listed.iter().find(|l| l.special == Some(tag));
    let mut out = vec![Folder { role: Role::Inbox, name: "INBOX".into() }];
    for (role, names) in NAMES {
        let hit = match role {
            Role::Archive => special("archive").or_else(|| special("all")),
            _ => special(role.as_str()),
        }
        .or_else(|| by_name(listed, names));
        if let Some(l) = hit {
            out.push(Folder { role, name: l.name.clone() });
        }
    }
    out
}

/// Nome para criar o Arquivo: segue o prefixo "INBOX." quando o servidor organiza tudo sob a Caixa de entrada.
pub fn archive_name(listed: &[Listed], delimiter: Option<&str>) -> String {
    match delimiter {
        Some(d) if listed.iter().any(|l| l.name.starts_with(&format!("INBOX{d}"))) => format!("INBOX{d}Archive"),
        _ => "Archive".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(name: &str, special: Option<&'static str>) -> Listed {
        Listed { name: name.into(), special }
    }

    fn name(f: &[Folder], r: Role) -> Option<&str> {
        f.iter().find(|x| x.role == r).map(|x| x.name.as_str())
    }

    #[test]
    fn gmail_usa_special_use_e_all_como_arquivo() {
        let f = pick(&[
            l("INBOX", None),
            l("[Gmail]/E-mails enviados", Some("sent")),
            l("[Gmail]/Todos os e-mails", Some("all")),
            l("[Gmail]/Lixeira", Some("trash")),
            l("[Gmail]/Spam", Some("junk")),
        ]);
        assert_eq!(name(&f, Role::Inbox), Some("INBOX"));
        assert_eq!(name(&f, Role::Sent), Some("[Gmail]/E-mails enviados"));
        assert_eq!(name(&f, Role::Archive), Some("[Gmail]/Todos os e-mails"));
        assert_eq!(name(&f, Role::Trash), Some("[Gmail]/Lixeira"));
        assert_eq!(name(&f, Role::Junk), Some("[Gmail]/Spam"));
    }

    #[test]
    fn archive_ganha_de_all() {
        let f = pick(&[l("All", Some("all")), l("Arquivo", Some("archive"))]);
        assert_eq!(name(&f, Role::Archive), Some("Arquivo"));
    }

    #[test]
    fn sem_atributo_usa_nomes_comuns_e_prefixo_inbox() {
        let f = pick(&[l("INBOX", None), l("INBOX.Sent", None), l("Itens Excluídos", None), l("junk", None)]);
        assert_eq!(name(&f, Role::Sent), Some("INBOX.Sent"));
        assert_eq!(name(&f, Role::Trash), Some("Itens Excluídos"));
        assert_eq!(name(&f, Role::Junk), Some("junk"));
        assert_eq!(name(&f, Role::Archive), None);
    }

    #[test]
    fn nome_do_arquivo_a_criar() {
        assert_eq!(archive_name(&[l("INBOX", None), l("INBOX.Sent", None)], Some(".")), "INBOX.Archive");
        assert_eq!(archive_name(&[l("INBOX", None), l("Sent", None)], Some("/")), "Archive");
        assert_eq!(archive_name(&[], None), "Archive");
    }

    #[test]
    fn papel_ida_e_volta() {
        for r in Role::ALL {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::parse("drafts"), None);
    }
}
