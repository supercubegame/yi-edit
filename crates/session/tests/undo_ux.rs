//! 会话层的撤销体验：上层真的接了分组与封口，而不是只在 core 里有规则。
use std::fs;
use std::path::PathBuf;
use yi_edit_core::Pos;
use yi_edit_session::Editor;

struct Tmp(PathBuf);
impl Tmp { fn new() -> Self { let d=std::env::temp_dir().join(format!("yi-undoux-{}",std::process::id())); let _=fs::remove_dir_all(&d); fs::create_dir_all(&d).unwrap(); Self(d) } fn file(&self,t:&str)->PathBuf { let p=self.0.join("a.txt"); fs::write(&p,t.as_bytes()).unwrap(); p } }
impl Drop for Tmp { fn drop(&mut self){let _=fs::remove_dir_all(&self.0);} }

fn depth(e:&Editor)->usize { e.doc().map(|d|d.undo_depth()).unwrap_or(0) }
fn type_text(e:&mut Editor,s:&str){for c in s.chars(){assert!(e.insert_text(&c.to_string()));}}

#[test]
fn session_typing_is_one_group_and_commit_splits_it(){
 let t=Tmp::new(); let p=t.file(""); let mut e=Editor::open(&p).unwrap();
 type_text(&mut e,"ab"); assert_eq!(depth(&e),1);
 e.commit_undo_group(); type_text(&mut e,"cd"); assert_eq!(depth(&e),2);
 e.doc_mut().unwrap().undo(); assert_eq!(e.doc().unwrap().to_text(),"ab");
 e.doc_mut().unwrap().undo(); assert_eq!(e.doc().unwrap().to_text(),"");
}

#[test]
fn session_paste_over_selection_is_one_group(){
 let t=Tmp::new(); let p=t.file("one two"); let mut e=Editor::open(&p).unwrap();
 e.anchor=Some(Pos::new(0,4)); e.cursor=Pos::new(0,7);
 assert!(e.insert_text("TWO")); assert_eq!(depth(&e),1);
 e.doc_mut().unwrap().undo(); assert_eq!(e.doc().unwrap().to_text(),"one two");
}
