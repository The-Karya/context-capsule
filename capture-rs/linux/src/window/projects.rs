// context-capsule — linux: Projects — each project with the words it's found by; Edit (name and clues) and
// Delete (asked first; its capsules stay). Saved to projects.json at once; the engine reads it again by itself.

use super::widgets::{self, label, row, vbox};
use super::Ctx;
use crate::ui;
use adw::prelude::*;
use context_capsule_core::projects::Project;
use context_capsule_core::settings;
use context_capsule_core::window as words;
use context_capsule_core::Projects;
use std::rc::{Rc, Weak};

pub struct ProjectsPage {
    ctx: Rc<Ctx>,
    pub root: gtk::Box,
    list: gtk::Box,
    me: std::cell::RefCell<Weak<ProjectsPage>>,
}

impl ProjectsPage {
    pub fn new(ctx: &Rc<Ctx>) -> Rc<ProjectsPage> {
        let root = vbox(16);
        root.append(&label(words::Section::Projects.title(), &["title-1"]));
        root.append(&label(words::PROJECTS_INTRO, &["dim-label"]));
        let list = vbox(0);
        list.set_margin_top(8);
        root.append(&list);
        let page = Rc::new(ProjectsPage { ctx: ctx.clone(), root, list, me: Default::default() });
        *page.me.borrow_mut() = Rc::downgrade(&page);
        page
    }

    pub fn refresh(&self) {
        widgets::clear(&self.list);
        let projects = Projects::load(&self.ctx.paths.projects());
        if projects.is_empty() {
            self.list.append(&label(words::NO_PROJECTS, &["dim-label"]));
            return;
        }
        let group = adw::PreferencesGroup::new();
        for p in projects {
            let r = row(&p.name, &words::found_by(&p));
            r.set_subtitle_lines(2);
            let edit = widgets::flat_button(words::EDIT);
            let delete = widgets::flat_button(words::DELETE);
            let (me, project) = (self.me.borrow().clone(), p.clone());
            edit.connect_clicked(move |_| {
                if let Some(page) = me.upgrade() {
                    page.edit(&project);
                }
            });
            let (me, project) = (self.me.borrow().clone(), p.clone());
            delete.connect_clicked(move |_| {
                if let Some(page) = me.upgrade() {
                    page.delete(&project);
                }
            });
            r.add_suffix(&edit);
            r.add_suffix(&delete);
            group.add(&r);
        }
        self.list.append(&group);
    }

    fn edit(&self, p: &Project) {
        let d = adw::MessageDialog::new(Some(&self.ctx.win), Some(ui::EDIT_PROJECT), None);
        let name = gtk::Entry::new();
        name.set_text(&p.name);
        name.set_activates_default(true);
        let clues = gtk::Entry::new();
        clues.set_text(&p.clues.join(", "));
        clues.set_activates_default(true);
        let form = vbox(6);
        form.append(&label(words::NAME_LABEL, &["caption-heading"]));
        form.append(&name);
        let clue_label = label(words::CLUES_LABEL, &["caption-heading"]);
        clue_label.set_margin_top(8);
        form.append(&clue_label);
        form.append(&clues);
        d.set_extra_child(Some(&form));
        d.add_response("cancel", words::CANCEL);
        d.add_response("save", words::SAVE);
        d.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("save"));
        d.set_close_response("cancel");
        let (me, id) = (self.me.borrow().clone(), p.id.clone());
        d.connect_response(None, move |_, response| {
            let Some(page) = me.upgrade() else { return };
            if response != "save" {
                return;
            }
            if settings::save_project_edit(&page.ctx.paths.projects(), &id, &name.text(), &clues.text()).is_err() {
                page.ctx.toast(ui::CANT_SAVE);
            }
            page.refresh();
        });
        d.present();
    }

    fn delete(&self, p: &Project) {
        let d = adw::MessageDialog::new(Some(&self.ctx.win), Some(&words::delete_question(p)), Some(words::DELETE_DETAIL));
        d.add_response("cancel", words::CANCEL);
        d.add_response("delete", words::DELETE);
        d.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        let (me, id) = (self.me.borrow().clone(), p.id.clone());
        d.connect_response(None, move |_, response| {
            let Some(page) = me.upgrade() else { return };
            if response == "delete" && settings::delete_project(&page.ctx.paths.projects(), &id).is_err() {
                page.ctx.toast(ui::CANT_SAVE);
            }
            page.refresh();
        });
        d.present();
    }
}
