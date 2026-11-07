comptime PLACE_TARGET_CATALOG = """new-session	new	0	0	0		.
list-sessions	ls	0	0	0		.
rename-session	rename	3	1	1	new name	.
kill-session		3	0	0		.
attach-session	attach	5	0	0		.
has-session	has	3	0	0		.
suspend-client	suspendc	2	0	0		.
detach-client	detach	2	0	0		.
list-clients	lsc	3	0	0		.
refresh-client	refresh	2	0	1		.
new-window	neww	4	0	0		.
list-windows	lsw	3	0	0		.
rename-window	renamew	4	1	1	new name	.
select-window	selectw	4	0	0		.
next-window	next	3	0	0		.
previous-window	prev	3	0	0		.
last-window	last	3	0	0		.
kill-window	killw	4	0	0		.
move-window	movew	4	0	0		.
swap-window	swapw	4	0	0		.
find-window	findw	5	1	1	match string	.
split-window	splitw	5	0	0		.
new-pane	newp	5	0	0		.
select-pane-kind		5	0	0		.
break-pane	breakp	4	0	0		.
join-pane	joinp	5	0	0		.
move-pane	movep	5	0	0		.
set-browser-url		5	0	1		.
set-browser-tabs		5	0	0		.
set-browser-profile		5	0	1		.
set-editor-path		5	0	1		.
select-pane	selectp	5	0	0		.
last-pane	lastp	4	0	0		.
swap-pane	swapp	5	0	0		.
list-panes	lsp	4	0	0		.
resize-pane	resizep	5	0	1		.
resize-window	resizew	4	0	1		.
select-layout	selectl	4	0	1		.
next-layout	nextl	4	0	0		.
previous-layout	prevl	4	0	0		.
rotate-window	rotatew	4	0	0		.
kill-pane	killp	5	0	0		.
respawn-pane	respawnp	5	0	0		.
respawn-window	respawnw	4	0	0		.
send-keys	send	5	0	0	keys	.
send-prefix		5	0	0		.
copy-mode		5	0	0		.
copy-mode-search-prompt		5	0	0		.
command-prompt		2	0	1		.
choose-tree		5	0	1		.
choose-client		5	0	1		.
focus-sidebar		5	0	0		.
choose-path		5	0	0		.
choose-buffer		5	0	1		.
display-message	display	5	0	1		.
show-messages	showmsgs	2	0	0		.
display-panes	displayp	5	0	1		.
clear-history	clearhist	5	0	0		.
customize-mode		5	0	0		.
clock-mode		5	0	0		.
switch-mode		5	0	1		.
bind-key	bind	0	1	0	key	.
unbind-key	unbind	0	0	1		.
list-keys	lsk	0	0	1		.
list-commands	lscm	0	0	1		.
set-hook		5	0	0		.
show-hooks		5	0	1		.
set-option	set	2	1	0	option	.
set-window-option	setw	4	1	0	option	.
show-options	show	2	0	1		.
show-window-options	showw	4	0	1		.
set-environment	setenv	3	1	0	variable	.
show-environment	showenv	3	0	1		.
source-file	source	5	1	0	path ...	.
reload-config		0	0	0		.
import-tmux-config		0	0	1		.
start-server	start	0	0	0		.
server-access		0	0	1		.
kill-server		0	0	0		.
inspect		5	0	0		.
events		2	0	0		.
wait-pane		5	0	0		.
run-pane		5	0	0		.
agent-catalog		5	0	0		.
providers		0	0	0		.
capture-pane	capturep	5	0	0		.
agent-send		5	0	0		.
agent-respond		5	0	1		.
flip-agent		5	0	0		.
send-last-output		5	0	0		.
show-last-output		5	0	0		.
send-text		5	0	0		.
capture-browser		5	0	0		.
html		5	0	1		.
debug-marker		0	0	0		.
tools		0	0	1		.
fleet		0	0	0		.
list-hosts		0	0	0		.
clear-prompt-history	clearphist	0	0	0		.
show-prompt-history	showphist	0	0	0		.
if-shell	if	5	2	0	shell command command	.
run-shell	run	5	0	0		.
delete-buffer	deleteb	0	0	0		.
list-buffers	lsb	0	0	0		.
load-buffer	loadb	2	1	1	path	.
paste-buffer	pasteb	5	0	0		.
save-buffer	saveb	0	1	1	path	.
set-buffer	setb	2	0	1		.
show-buffer	showb	0	0	0		.
wait-for	wait	0	1	1	name	.
pipe-pane	pipep	5	0	1		.
display-popup	popup	5	0	0		.
display-menu	menu	5	1	0	name ...	.
confirm-before	confirm	2	1	1	command	.
lock-client	lockc	2	0	0		.
lock-server	lock	0	0	0		.
lock-session	locks	3	0	0		.
switch-client	switchc	3	0	0		."""


@fieldwise_init
struct PaletteTargetTraits(Copyable, Movable):
    var name: String
    var supported: Bool
    var kind: Optional[UInt8]
    var minimum: Int
    var single: Bool
    var text_label: String


def target_traits(name: String) raises -> Optional[PaletteTargetTraits]:
    for line in String(PLACE_TARGET_CATALOG).split("\n"):
        var fields = line.split("\t")
        if len(fields) != 7 or not (fields[6] == "."):
            raise Error("invalid palette target metadata")
        var matching = fields[0] == name
        for alias in fields[1].split(","):
            matching = matching or alias.byte_length() > 0 and alias == name
        if matching:
            var code = Int(fields[2])
            var kind: Optional[UInt8] = None
            if code >= 2:
                kind = UInt8(code - 2)
            return PaletteTargetTraits(String(fields[0]), code != 0, kind, Int(fields[3]), fields[4] == "1", String(fields[5]))
    return None
