use ala::{App, Context, Entity, EventEmitter};
use zz_client::sidebar::SidebarTarget;
use zz_ui::command::PaletteHostId;

use crate::{app::HostPage, connection::Connection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostRequest {
    Open(PaletteHostId, SidebarTarget),
    Connect(PaletteHostId),
}

#[derive(Default)]
pub(crate) struct Roster {
    hosts: Vec<HostPage>,
}

impl EventEmitter<HostRequest> for Roster {}

impl Roster {
    pub(crate) fn sync(&mut self, hosts: Vec<HostPage>, cx: &mut Context<Self>) {
        if self.hosts != hosts {
            self.hosts = hosts;
            cx.notify();
        }
    }

    pub(crate) fn hosts(&self) -> &[HostPage] {
        &self.hosts
    }

    pub(crate) fn connection(&self, host: PaletteHostId) -> Option<&Entity<Connection>> {
        self.hosts
            .iter()
            .find(|page| page.id == host)?
            .connection
            .as_ref()
    }

    pub(crate) fn id_of(&self, connection: &Entity<Connection>) -> Option<PaletteHostId> {
        self.hosts
            .iter()
            .find(|page| page.connection.as_ref() == Some(connection))
            .map(|page| page.id)
    }

    pub(crate) fn request(roster: &Entity<Self>, request: HostRequest, cx: &mut App) {
        roster.update(cx, |_, cx| cx.emit(request));
    }
}
