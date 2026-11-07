comptime CLI_CATALOG = """agent-catalog		1	0	2	[-t target-pane] provider request-id	-t:1
agent-respond		1	0	1	[-t target-pane] [--allow] [--deny] [--option option-id] [request-id]	-t:1,--allow:0,--deny:0,--option:1
agent-send		1	0	-1	[-t target-pane] [--submit] [--wait] [--notify] [--progress] [--timeout seconds] [--on-block wait|fail|allow|deny] [--json] [--final] [--context context] [text ...]	-t:1,--submit:0,--wait:0,--notify:0,--json:0,--final:0,--progress:0,--timeout:1,--on-block:1,--context:1
attach-session	attach	0	0	0	[-dErx] [-c working-directory] [-f flags] [-t target-session]	-d:0,-E:0,-r:0,-c:1,-t:1,-f:1,-x:0
bind-key	bind	0	1	-1	[-nr] [-T key-table] [-N note] key [command [argument ...]]	-T:1,-N:1,-n:0,-r:0
break-pane	breakp	0	0	0	[-abdPW] [-F format] [-n window-name] [-s src-pane] [-t dst-window] [-x width] [-y height] [-X x-position] [-Y y-position]	-n:1,-s:1,-t:1,-a:0,-b:0,-d:0,-P:0,-F:1,-W:0,-x:1,-y:1,-X:1,-Y:1
capture-browser		1	0	0	[-t target-pane] [-o output-path]	-t:1,-o:1
capture-pane	capturep	0	0	0	[-aCeFHIJLMNpPqRT] [-b buffer-name] [-E end-line] [-S start-line] [-t target-pane]	-a:0,-C:0,-e:0,-F:0,-H:0,-J:0,-L:0,-M:0,-N:0,-p:0,-q:0,-T:0,-b:1,-E:1,-S:1,-t:1,-I:0,-P:0,-R:0
choose-buffer		0	0	1	[-kNrZ] [-F format] [-f filter] [-K key-format] [-O sort-order] [-t target-pane] [template]	-t:1,-Z:0,-F:1,-f:1,-K:1,-k:0,-N:0,-O:1,-r:0,-y:0
choose-client		0	0	1	[-hikNrZ] [-F format] [-f filter] [-K key-format] [-O sort-order] [-t target-pane] [template]	-t:1,-Z:0,-F:1,-f:1,-h:0,-i:0,-K:1,-k:0,-N:0,-O:1,-r:0,-y:0
choose-path		1	0	0	[-t target-pane] [-c start-directory]	-t:1,-c:1
choose-tree		0	0	1	[-GhkNrswZ] [-F format] [-f filter] [-K key-format] [-O sort-order] [-t target-pane] [template]	-t:1,-s:0,-w:0,-Z:0,-F:1,-f:1,-G:0,-h:0,-K:1,-k:0,-N:0,-O:1,-r:0,-y:0
clear-history	clearhist	0	0	0	[-H] [-t target-pane]	-t:1,-H:0
clear-prompt-history	clearphist	0	0	0	[-T prompt-type]	-T:1
clock-mode		0	0	0	[-t target-pane]	-t:1
command-prompt		0	0	1	[-1CbeFiklNP] [-I inputs] [-p prompts] [-t target-client] [-T prompt-type] [template]	-I:1,-p:1,-t:1,-T:1,-1:0,-b:0,-C:0,-e:0,-F:0,-i:0,-k:0,-l:0,-N:0,-P:0
confirm-before	confirm	0	1	1	[-by] [-c confirm-key] [-p prompt] [-t target-client] command	-b:0,-y:0,-c:1,-p:1,-t:1
copy-mode		0	0	0	[-dekHMqSu] [-s src-pane] [-t target-pane]	-t:1,-s:1,-d:0,-u:0,-e:0,-k:0,-H:0,-M:0,-q:0,-S:0
copy-mode-search-prompt		1	0	0	[-b] [-t target-pane]	-t:1,-b:0
customize-mode		0	0	0	[-kNZ] [-F format] [-f filter] [-t target-pane]	-F:1,-N:0,-Z:0,-f:1,-k:0,-t:1,-y:0
debug-marker		1	0	-1	[note ...]	
delete-buffer	deleteb	0	0	0	[-b buffer-name]	-b:1
detach-client	detach	0	0	0	[-aP] [-E shell-command] [-s target-session] [-t target-client]	-a:0,-s:1,-t:1,-E:1,-P:0
display-menu	menu	0	1	-1	[-MO] [-b border-lines] [-c target-client] [-C starting-choice] [-H selected-style] [-s style] [-S border-style] [-t target-pane] [-T title] [-x position] [-y position] name [key] [command] ...	-M:0,-O:0,-b:1,-c:1,-C:1,-H:1,-s:1,-S:1,-t:1,-T:1,-x:1,-y:1
display-message	display	0	0	1	[-aCIjlNpv] [-c target-client] [-d delay] [-F format] [-t target-pane] [message]	-p:0,-C:0,-c:1,-d:1,-F:1,-l:0,-N:0,-t:1,-a:0,-v:0,-I:0,-j:0
display-panes	displayp	0	0	1	[-kNZ] [-d duration] [-s source-window] [-t target-pane] [template]	-d:1,-N:0,-Z:0,-k:0,-s:1,-t:1
display-popup	popup	0	0	-1	[-BCEkN] [-b border-lines] [-c target-client] [-d start-directory] [-e environment] [-h height] [-s style] [-S border-style] [-t target-pane] [-T title] [-w width] [-x position] [-y position] [shell-command [argument ...]]	-B:0,-C:0,-E:0,-k:0,-N:0,-b:1,-c:1,-d:1,-e:1,-h:1,-s:1,-S:1,-t:1,-T:1,-w:1,-x:1,-y:1
events		1	0	0	[-t target]	-t:1
find-window	findw	0	1	1	[-CiNrTZ] [-t target-pane] match-string	-C:0,-i:0,-N:0,-r:0,-T:0,-Z:0,-t:1
fleet		1	0	-1	add NAME DEST | remove NAME | install [--restart-daemon] (-a | NAME...) | list | key | sync | trust KEY	--restart-daemon:0
flip-agent		1	0	0	[-t target-pane] [--now] [--when-idle] [--cancel]	-t:1,--now:0,--when-idle:0,--cancel:0
focus-sidebar		1	0	0	[-t target-pane]	-t:1
has-session	has	0	0	0	[-t target-session]	-t:1
html		1	0	1	[--float] [--reply] [-t target-pane] path|-	-t:1,--float:0,--reply:0
if-shell	if	0	2	3	[-bF] [-t target-pane] shell-command command [command]	-b:0,-F:0,-t:1
import-tmux-config		1	0	1	[path]	
inspect		1	0	0	[-a] [-t target-pane] [--json]	-a:0,-t:1,--json:0
join-pane	joinp	0	0	0	[-bdfhv] [-l size] [-s src-pane] [-t dst-pane]	-l:1,-p:1,-s:1,-t:1,-b:0,-d:0,-f:0,-h:0,-v:0
kill-pane	killp	0	0	0	[-a] [-f filter] [-t target-pane]	-t:1,-a:0,-f:1
kill-server		0	0	0		
kill-session		0	0	0	[-aCg] [-f filter] [-t target-session]	-t:1,-a:0,-C:0,-g:0,-f:1
kill-window	killw	0	0	0	[-a] [-f filter] [-t target-window]	-t:1,-a:0,-f:1
last-pane	lastp	0	0	0	[-deZ] [-t target-window]	-t:1,-d:0,-e:0,-Z:0
last-window	last	0	0	0	[-t target-session]	-t:1
link-window	linkw	0	0	0	[-abdk] [-s src-window] [-t dst-window]	-a:0,-b:0,-d:0,-k:0,-s:1,-t:1
list-buffers	lsb	0	0	0	[-F format] [-f filter] [-O order]	-F:1,-f:1,-O:1,-r:0
list-clients	lsc	0	0	0	[-F format] [-f filter] [-O order][-t target-session]	--json:8,-F:1,-t:1,-f:1,-O:1,-r:0
list-commands	lscm	0	0	1	[-F format] [command]	-F:1
list-hosts		1	0	0	[-F format] [--json]	-F:1,--json:0
list-keys	lsk	0	0	1	[-1aNr] [-F format] [-O order] [-P prefix-string][-T key-table] [key]	-1:0,-F:1,-O:1,-P:1,-T:1,-a:0,-N:0,-r:0
list-panes	lsp	0	0	0	[-asr] [-F format] [-f filter] [-O order][-t target-window]	--json:8,-t:1,-F:1,-f:1,-O:1,-a:0,-r:0,-s:0
list-sessions	ls	0	0	0	[-r] [-F format] [-f filter] [-O order]	--json:8,-F:1,-f:1,-O:1,-r:0
list-windows	lsw	0	0	0	[-ar] [-F format] [-f filter] [-O order][-t target-session]	--json:8,-t:1,-F:1,-a:0,-f:1,-O:1,-r:0
load-buffer	loadb	0	1	1	[-b buffer-name] [-t target-client] path	-b:1,-t:1,-w:0
lock-client	lockc	0	0	0	[-t target-client]	-t:1
lock-server	lock	0	0	0		
lock-session	locks	0	0	0	[-t target-session]	-t:1
move-pane	movep	0	0	0	[-bdfhMv] [-D lines] [-l size] [-L columns] [-P position] [-R columns] [-s src-pane] [-t dst-pane] [-U lines] [-X x-position] [-Y y-position] [-z z-index]	-l:1,-s:1,-t:1,-b:0,-d:0,-f:0,-h:0,-v:0,-D:6,-L:6,-P:1,-R:6,-U:6,-X:1,-Y:1,-z:1,-M:0
move-window	movew	0	0	0	[-abdkr] [-s src-window] [-t dst-window]	-a:0,-b:0,-d:0,-k:0,-r:0,-s:1,-t:1
new-pane	newp	0	0	-1	[-AbCDefhIkKLMOPvWZ] [-B border-lines] [-c start-directory] [-e environment] [-F format] [-l size] [-m message] [-p percentage] [-s style] [-S active-border-style] [-R inactive-border-style] [-T title] [-x width] [-y height] [-X x-position] [-Y y-position] [-t target-pane] [shell-command [argument ...]]	--kind:9,--profile:9,-t:1,-A:0,-C:0,-D:0,-K:0,-L:0,-M:0,-O:0,-x:1,-y:1,-X:1,-Y:1,-l:1,-p:1,-c:1,-b:0,-d:0,-f:0,-h:0,-P:0,-F:1,-v:0,-e:1,-E:0,-I:0,-k:0,-m:1,-R:1,-s:1,-S:1,-T:1,-W:0,-Z:0,-B:1
new-session	new	0	0	-1	[-AdDEPX] [-c start-directory] [-e environment] [-F format] [-f flags] [-n window-name] [-s session-name] [-t target-session] [-x width] [-y height] [shell-command [argument ...]]	-d:0,-A:0,-D:0,-E:0,-P:0,-X:0,-s:1,-n:1,-c:1,-t:1,-e:1,-F:1,-f:1,-x:1,-y:1
new-window	neww	0	0	-1	[-abdEkPS] [-c start-directory] [-e environment] [-F format] [-n window-name] [-t target-window] [shell-command [argument ...]]	--kind:9,--profile:9,-t:1,-n:1,-c:1,-d:0,-a:0,-b:0,-k:0,-P:0,-F:1,-S:0,-E:0,-e:1
next-layout	nextl	0	0	0	[-t target-window]	-t:1
next-window	next	0	0	0	[-a] [-t target-session]	-t:1,-a:0
paste-buffer	pasteb	0	0	0	[-dprS] [-s separator] [-b buffer-name] [-t target-pane]	-d:0,-p:0,-r:0,-S:0,-s:1,-b:1,-t:1
pipe-pane	pipep	0	0	1	[-IOo] [-t target-pane] [shell-command]	-I:0,-O:0,-o:0,-t:1
previous-layout	prevl	0	0	0	[-t target-window]	-t:1
previous-window	prev	0	0	0	[-a] [-t target-session]	-t:1,-a:0
providers		1	0	0	[-u provider]	-u:1
refresh-client	refresh	0	0	1	[-cDlLRSU] [-A pane:state] [-B name:what:format] [-C XxY] [-f flags] [-r pane:report] [-t target-client] [adjustment]	-c:0,-D:0,-l:0,-L:0,-R:0,-S:0,-U:0,-A:1,-B:1,-C:1,-F:1,-f:1,-r:1,-t:1
reload-config		1	0	0		
rename-session	rename	0	1	1	[-t target-session] new-name	-t:1
rename-window	renamew	0	1	1	[-t target-window] new-name	-t:1
resize-pane	resizep	0	0	1	[-MTZ] [-D lines] [-L columns] [-R columns] [-U lines] [-x width] [-y height] [-t target-pane]	-t:1,-x:1,-y:1,-D:6,-L:6,-R:6,-U:6,-Z:0,-M:0,-T:0
resize-window	resizew	0	0	1	[-aADLRU] [-x width] [-y height] [-t target-window] [adjustment]	-t:1,-x:1,-y:1,-D:0,-L:0,-R:0,-U:0,-a:0,-A:0
respawn-pane	respawnp	0	0	-1	[-Ek] [-c start-directory] [-e environment] [-t target-pane] [shell-command [argument ...]]	-t:1,-c:1,-e:1,-E:0,-k:0
respawn-window	respawnw	0	0	-1	[-Ek] [-c start-directory] [-e environment] [-t target-window] [shell-command [argument ...]]	-t:1,-c:1,-e:1,-E:0,-k:0
rotate-window	rotatew	0	0	0	[-DUZ] [-t target-window]	-t:1,-D:0,-U:0,-Z:0
run-pane		1	0	-1	[-t target-pane] [--timeout SECS] COMMAND...	-t:1,--timeout:1
run-shell	run	0	0	-1	[-bCE] [-c start-directory] [-d delay] [-t target-pane] [shell-command [argument ...]]	-b:0,-C:0,-E:0,-c:1,-d:1,-t:1,-s:1
save-buffer	saveb	0	1	1	[-a] [-b buffer-name] path	-a:0,-b:1
select-layout	selectl	0	0	1	[-Enop] [-t target-pane] [layout-name]	-t:1,-E:0,-n:0,-o:0,-p:0
select-pane	selectp	0	0	0	[-DdeLlMmRUZ] [-T title] [-t target-pane]	-t:1,-T:1,-P:1,-D:0,-L:0,-M:0,-R:0,-U:0,-Z:0,-d:0,-e:0,-g:0,-l:0,-m:0
select-pane-kind		1	0	-1	[-c start-directory] [-t target-pane] [--resume session-id] [--fork] [--at message-id] [--model model] [--effort effort] pane-kind [shell-command [argument ...]]	-t:1,-c:1,--resume:1,--fork:0,--at:1,--model:1,--effort:1
select-window	selectw	0	0	0	[-lnpT] [-t target-window]	-t:1,-l:0,-n:0,-p:0,-T:0
send-keys	send	0	0	-1	[-FHKlMRX] [-c target-client] [-N repeat-count] [-t target-pane] [key ...]	-t:1,-N:1,-X:0,-H:0,-F:0,-l:0,-c:1,-K:0,-M:0,-R:0
send-last-output		1	0	0	[-t target-pane]	-t:1
send-prefix		0	0	0	[-2] [-t target-pane]	-2:0,-t:1
send-text		1	0	-1	[-t target-pane] [--no-enter] [--timeout seconds] text ...	-t:1,--no-enter:0,--timeout:1
server-access		0	0	1	[-adglrw] [-t target-pane] [user|group]	-a:0,-d:0,-g:0,-l:0,-r:0,-w:0
set-browser-profile		1	0	1	[-t target-pane] profile	-t:1
set-browser-tabs		1	0	-1	[-a active-tab-index] [-t target-pane] url ...	-t:1,-a:1
set-browser-url		1	0	1	[-t target-pane] url	-t:1
set-buffer	setb	0	0	1	[-aw] [-b buffer-name] [-n new-buffer-name] [-t target-client] [data]	-a:0,-b:1,-n:1,-t:1,-w:0
set-editor-path		1	0	1	[-t target-pane] [path]	-t:1
set-environment	setenv	0	1	2	[-Fhgru] [-t target-session] variable [value]	-F:0,-g:0,-h:0,-r:0,-t:1,-u:0
set-hook		0	0	2	[-agpERTuw] [-B name:what:format] [-t target-pane] [hook] [command]	-B:1,-t:1,-a:0,-g:0,-p:0,-R:0,-u:0,-w:0,-E:0,-T:0
set-option	set	0	1	2	[-aFgopqsuUw] [-t target-pane] option [value]	-t:1,-a:0,-F:0,-g:0,-o:0,-p:0,-q:0,-s:0,-u:0,-U:0,-w:0
set-window-option	setw	0	1	2	[-aFgoqu] [-t target-window] option [value]	-t:1,-a:0,-F:0,-g:0,-o:0,-q:0,-u:0
show-buffer	showb	0	0	0	[-b buffer-name]	-b:1
show-environment	showenv	0	0	1	[-hgs] [-t target-session] [variable]	-g:0,-h:0,-s:0,-t:1
show-hooks		0	0	1	[-Bgpw] [-F format] [-t target-pane] [hook]	-B:0,-F:1,-t:1,-g:0,-p:0,-w:0
show-last-output		1	0	0	[-t target-pane]	-t:1
show-messages	showmsgs	0	0	0	[-JT] [-t target-client]	-J:0,-T:0,-t:1
show-options	show	0	0	1	[-AgHpqsvw] [-F format] [-t target-pane] [option]	--json:8,-A:0,-F:1,-g:0,-H:0,-p:0,-q:0,-s:0,-t:1,-v:0,-w:0
show-prompt-history	showphist	0	0	0	[-T prompt-type]	-T:1
show-window-options	showw	0	0	1	[-gv] [-F format] [-t target-window] [option]	-F:1,-g:0,-t:1,-v:0
source-file	source	0	1	-1	[-Fnqv] [-t target-pane] path ...	-F:0,-n:0,-q:0,-t:1,-v:0
split-window	splitw	0	0	-1	[-bdefhIklPvWZ] [-B border-lines] [-c start-directory] [-e environment] [-F format] [-l size] [-m message] [-p percentage] [-s style] [-S active-border-style] [-R inactive-border-style] [-T title] [-t target-pane] [shell-command [argument ...]]	--kind:9,--profile:9,-t:1,-l:1,-p:1,-c:1,-b:0,-d:0,-f:0,-h:0,-P:0,-F:1,-v:0,-e:1,-E:0,-I:0,-k:0,-m:1,-R:1,-s:1,-S:1,-T:1,-W:0,-Z:0,-B:1
start-server	start	0	0	0		
suspend-client	suspendc	0	0	0	[-t target-client]	-t:1
swap-pane	swapp	0	0	0	[-dDUZ] [-s src-pane] [-t dst-pane]	-s:1,-t:1,-d:0,-D:0,-U:0,-Z:0
swap-window	swapw	0	0	0	[-d] [-s src-window] [-t dst-window]	-d:0,-s:1,-t:1
switch-client	switchc	0	0	0	[-ElnprZ] [-c target-client] [-t target-session] [-T key-table] [-O order]	-E:0,-l:0,-n:0,-p:0,-r:0,-Z:0,-c:1,-t:1,-T:1,-O:1,-F:0
switch-mode		0	0	1	[-kswZ] [-F format] [-t target-pane] [command]	-F:1,-Z:0,-k:0,-s:0,-t:1,-w:0
tools		1	0	1	[--skill] [--context] [--install-hooks] [--remove-hooks] [--fork-chat] [section]	--skill:0,--context:0,--install-hooks:0,--remove-hooks:0,--fork-chat:0
unbind-key	unbind	0	0	1	[-anq] [-T key-table] key	-T:1,-a:0,-n:0,-q:0
unlink-window	unlinkw	0	0	0	[-k] [-t target-window]	-k:0,-t:1
wait-for	wait	0	1	1	[-ELSUlv] [-F format] [-w waiter] name	-L:0,-S:0,-U:0,-E:0,-F:1,-l:0,-v:0,-w:1
wait-pane		1	0	0	[-t target-pane] [--idle MS] [--until TEXT] [--regex RE] [--exit] [--timeout SECS] [--tail N]	-t:1,--exit:0,--idle:1,--until:1,--regex:1,--timeout:1,--tail:1"""
