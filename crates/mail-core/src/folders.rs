//! Pastas padrão de cada conta: qual nome do servidor faz o papel de Enviados, Arquivo, Lixeira e Spam.

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

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

/// O servidor já organiza as pastas sob a Caixa de entrada ("INBOX." ou "INBOX/")?
fn under_inbox(listed: &[Listed], d: &str) -> bool {
    let prefix = format!("INBOX{d}");
    listed.iter().any(|l| l.name.starts_with(&prefix))
}

/// Nomes a tentar, em ordem, para criar uma pasta de sistema (ex.: "Sent", "Archive"). Primeiro segue o
/// prefixo "INBOX." quando o servidor já organiza tudo sob a Caixa de entrada; a outra forma fica de reserva,
/// porque servidores como o Courier recusam pasta fora de "INBOX." mesmo quando só existe a INBOX.
pub fn system_names(listed: &[Listed], delimiter: Option<&str>, leaf: &str) -> Vec<String> {
    let Some(d) = delimiter else { return vec![leaf.into()] };
    let prefixed = format!("INBOX{d}{leaf}");
    if under_inbox(listed, d) {
        vec![prefixed, leaf.into()]
    } else {
        vec![leaf.into(), prefixed]
    }
}

/// Rascunhos ficam locais; uma pasta de rascunhos do servidor não é pasta de usuário.
const DRAFT_NAMES: [&str; 3] = ["drafts", "draft", "rascunhos"];

/// Nome sem o prefixo "INBOX." ou "INBOX/" que alguns servidores põem em todas as pastas.
fn relative(name: &str) -> &str {
    name.strip_prefix("INBOX.").or_else(|| name.strip_prefix("INBOX/")).unwrap_or(name)
}

/// Pastas de sistema (Entrada, Enviados, Arquivo, Lixeira, Spam, Rascunhos): não renomeiam nem excluem.
pub fn is_system(name: &str, system: &[Folder]) -> bool {
    name.eq_ignore_ascii_case("INBOX")
        || system.iter().any(|f| f.name == name)
        || DRAFT_NAMES.contains(&relative(name).to_lowercase().as_str())
}

/// Pastas que o usuário pode gerenciar: o resto do LIST, sem as de sistema e sem as marcadas com SPECIAL-USE.
pub fn user_folders(listed: &[Listed], system: &[Folder]) -> Vec<String> {
    listed
        .iter()
        .filter(|l| l.special.is_none() && !is_system(&l.name, system))
        .filter(|l| !l.name.starts_with("[Gmail]") && !l.name.starts_with("[Google Mail]"))
        .map(|l| l.name.clone())
        .collect()
}

/// Texto digitado como nome de uma pasta: sem vazio, separadores nem curingas do IMAP.
fn clean_leaf(leaf: &str, delimiter: Option<&str>) -> Result<String> {
    let leaf = leaf.trim();
    let bad = leaf.is_empty()
        || leaf.chars().any(|c| matches!(c, '*' | '%' | '/' | '\\') || c.is_control())
        || delimiter.is_some_and(|d| leaf.contains(d));
    if bad {
        return Err(Error::Invalid("nome de pasta inválido: não use / \\ * % nem o separador do servidor".into()));
    }
    Ok(leaf.to_string())
}

/// Nome completo de uma pasta nova. Sem pai, segue o prefixo "INBOX." quando o servidor organiza tudo sob a Caixa de entrada.
pub fn new_name(listed: &[Listed], delimiter: Option<&str>, parent: Option<&str>, leaf: &str) -> Result<String> {
    let leaf = clean_leaf(leaf, delimiter)?;
    let name = match (delimiter, parent) {
        (Some(d), Some(p)) => format!("{p}{d}{leaf}"),
        (None, Some(_)) => return Err(Error::Invalid("este servidor não aceita subpastas".into())),
        (Some(d), None) if under_inbox(listed, d) => format!("INBOX{d}{leaf}"),
        _ => leaf,
    };
    if listed.iter().any(|l| l.name.eq_ignore_ascii_case(&name)) {
        return Err(Error::Invalid(format!("já existe uma pasta chamada {name}")));
    }
    Ok(name)
}

/// Nome completo depois de renomear: mesma pasta-pai, novo último trecho.
pub fn renamed(listed: &[Listed], delimiter: Option<&str>, old: &str, leaf: &str) -> Result<String> {
    let leaf = clean_leaf(leaf, delimiter)?;
    let name = match delimiter.and_then(|d| old.rsplit_once(d).map(|(parent, _)| format!("{parent}{d}{leaf}"))) {
        Some(n) => n,
        None => leaf,
    };
    if listed.iter().any(|l| l.name.eq_ignore_ascii_case(&name) && l.name != old) {
        return Err(Error::Invalid(format!("já existe uma pasta chamada {name}")));
    }
    Ok(name)
}

/// A pasta e as subpastas dela (nomes que começam com `name` + separador), da mais funda para a mais rasa.
pub fn with_children(listed: &[Listed], delimiter: Option<&str>, name: &str) -> Vec<String> {
    let prefix = delimiter.map(|d| format!("{name}{d}"));
    let mut out: Vec<String> = listed
        .iter()
        .filter(|l| l.name == name || prefix.as_deref().is_some_and(|p| l.name.starts_with(p)))
        .map(|l| l.name.clone())
        .collect();
    out.sort_by_key(|n| std::cmp::Reverse(n.len()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(name: &str, special: Option<&'static str>) -> Listed {
        Listed { name: name.into(), special }
    }

    fn system() -> Vec<Folder> {
        vec![Folder { role: Role::Inbox, name: "INBOX".into() }, Folder { role: Role::Sent, name: "INBOX.Sent".into() }]
    }

    #[test]
    fn pastas_de_usuario_sem_as_de_sistema() {
        let listed = [
            l("INBOX", None),
            l("INBOX.Sent", Some("sent")),
            l("INBOX.Drafts", Some("drafts")),
            l("INBOX.Rascunhos", None),
            l("INBOX.Projetos", None),
            l("INBOX.Projetos.Obra", None),
            l("[Gmail]/Important", None),
        ];
        assert_eq!(user_folders(&listed, &system()), ["INBOX.Projetos", "INBOX.Projetos.Obra"]);
        assert!(is_system("inbox", &system()) && is_system("INBOX.Sent", &system()) && is_system("Drafts", &system()));
        assert!(!is_system("INBOX.Projetos", &system()));
    }

    #[test]
    fn nome_de_pasta_nova_com_e_sem_pai() {
        let listed = [l("INBOX", None), l("INBOX.Projetos", None)];
        assert_eq!(new_name(&listed, Some("."), None, " Clientes ").unwrap(), "INBOX.Clientes");
        assert_eq!(new_name(&listed, Some("."), Some("INBOX.Projetos"), "Obra").unwrap(), "INBOX.Projetos.Obra");
        assert_eq!(new_name(&[l("INBOX", None)], Some("/"), None, "Clientes").unwrap(), "Clientes");
        assert!(new_name(&listed, Some("."), None, "projetos").is_err(), "repetida");
        assert!(new_name(&listed, Some("."), None, "a.b").is_err(), "com o separador");
        assert!(new_name(&listed, Some("."), None, "  ").is_err(), "vazia");
        assert!(new_name(&listed, None, Some("x"), "y").is_err(), "servidor sem hierarquia");
    }

    #[test]
    fn renomear_mantem_a_pasta_pai() {
        let listed = [l("INBOX.A", None), l("INBOX.A.B", None), l("INBOX.C", None)];
        assert_eq!(renamed(&listed, Some("."), "INBOX.A.B", "Z").unwrap(), "INBOX.A.Z");
        assert_eq!(renamed(&listed, Some("."), "INBOX.A", "Novo").unwrap(), "INBOX.Novo");
        assert_eq!(renamed(&listed, Some("."), "INBOX.A", "a").unwrap(), "INBOX.a", "só trocar maiúsculas vale");
        assert!(renamed(&listed, Some("."), "INBOX.A", "C").is_err());
    }

    #[test]
    fn filhas_saem_antes_da_pai() {
        let listed = [l("A", None), l("A/B", None), l("A/B/C", None), l("AB", None)];
        assert_eq!(with_children(&listed, Some("/"), "A"), ["A/B/C", "A/B", "A"]);
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
    fn nomes_para_criar_pasta_de_sistema() {
        assert_eq!(system_names(&[l("INBOX", None), l("INBOX.Sent", None)], Some("."), "Archive"), ["INBOX.Archive", "Archive"]);
        assert_eq!(system_names(&[l("INBOX", None), l("Sent", None)], Some("/"), "Archive"), ["Archive", "INBOX/Archive"]);
        // Só a INBOX: não dá para saber o namespace; tenta sem prefixo e depois com.
        assert_eq!(system_names(&[l("INBOX", None)], Some("."), "Sent"), ["Sent", "INBOX.Sent"]);
        assert_eq!(system_names(&[], None, "Sent"), ["Sent"]);
    }

    #[test]
    fn papel_ida_e_volta() {
        for r in Role::ALL {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::parse("drafts"), None);
    }
}
