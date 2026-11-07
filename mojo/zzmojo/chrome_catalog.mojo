comptime CHROME_CATALOG_SOURCE = "2261e7a07101c641cd3ba7bda9255cf8eb348600fc7a49808c5b55b5b0d42023"

comptime CHROME_ACTION_NAMES = """new-session
new-window
select-window
split-right
split-down
detach
toggle-sidebar
close-pane
open-settings
open-command-palette
ui-zoom-in
ui-zoom-out
ui-zoom-reset
sidebar-cancel
sidebar-confirm
sidebar-rename
sidebar-command-palette
sidebar-select-up
sidebar-select-down
sidebar-select-left
sidebar-select-right
sidebar-select-first
sidebar-select-last
terminal-font-increase
terminal-font-decrease
terminal-search
terminal-copy
terminal-select-all
terminal-clear-history
terminal-paste
browser-zoom-in
browser-zoom-out
browser-zoom-reset
browser-devtools
browser-new-tab
browser-next-tab
browser-previous-tab
browser-select-tab
browser-select-last-tab
browser-back
browser-forward
browser-reload
browser-focus-address
browser-element-selector
browser-undo
browser-redo
browser-cut
browser-copy
browser-paste
browser-paste-and-match-style
browser-select-all"""

comptime CHROME_PROFILE_DEFAULTS = """0	browser	C-+	7680
0	browser	C--	7936
0	browser	C-0	8192
0	browser	C-=	7680
0	browser	C-_	7936
0	sidebar	Down	4608
0	sidebar	Enter	3584
0	sidebar	Escape	3328
0	sidebar	Up	4352
0	sidebar	j	4608
0	sidebar	k	4352
0	sidebar	q	1536
0	sidebar	r	3840
1	browser	C-+	7680
1	browser	C--	7936
1	browser	C-0	8192
1	browser	C-1	9472
1	browser	C-2	9473
1	browser	C-3	9474
1	browser	C-4	9475
1	browser	C-5	9476
1	browser	C-6	9477
1	browser	C-7	9478
1	browser	C-8	9479
1	browser	C-9	9728
1	browser	C-=	7680
1	browser	C-NPage	8960
1	browser	C-PPage	9216
1	browser	C-S-Tab	9216
1	browser	C-S-c	11008
1	browser	C-S-i	8448
1	browser	C-S-v	12544
1	browser	C-S-z	11520
1	browser	C-Tab	8960
1	browser	C-a	12800
1	browser	C-c	12032
1	browser	C-l	10752
1	browser	C-r	10496
1	browser	C-t	8704
1	browser	C-v	12288
1	browser	C-w	1792
1	browser	C-x	11776
1	browser	C-y	11520
1	browser	C-z	11264
1	browser	F5	10496
1	browser	M-Left	9984
1	browser	M-Right	10240
1	sidebar	:	4096
1	sidebar	Down	4608
1	sidebar	End	5632
1	sidebar	Enter	3584
1	sidebar	Escape	3328
1	sidebar	G	5632
1	sidebar	Home	5376
1	sidebar	Left	4864
1	sidebar	Right	5120
1	sidebar	Up	4352
1	sidebar	g	5376
1	sidebar	h	4864
1	sidebar	j	4608
1	sidebar	k	4352
1	sidebar	l	5120
1	sidebar	q	3328
1	sidebar	r	3840
1	terminal	C-+	5888
1	terminal	C--	6144
1	terminal	C-=	5888
1	terminal	C-S-a	6912
1	terminal	C-S-c	6656
1	terminal	C-S-f	6400
1	terminal	C-S-v	7424
1	terminal	D-a	6912
1	terminal	D-c	6656
1	terminal	D-f	6400
1	terminal	D-v	7424
1	ui	C-+	2560
1	ui	C-,	2048
1	ui	C--	2816
1	ui	C-0	3072
1	ui	C-1	512
1	ui	C-2	513
1	ui	C-3	514
1	ui	C-4	515
1	ui	C-5	516
1	ui	C-6	517
1	ui	C-7	518
1	ui	C-8	519
1	ui	C-9	520
1	ui	C-=	2560
1	ui	C-S-k	2304
1	ui	C-S-p	2304
2	browser	C-S-Tab	9216
2	browser	C-Tab	8960
2	browser	D-+	7680
2	browser	D--	7936
2	browser	D-0	8192
2	browser	D-1	9472
2	browser	D-2	9473
2	browser	D-3	9474
2	browser	D-4	9475
2	browser	D-5	9476
2	browser	D-6	9477
2	browser	D-7	9478
2	browser	D-8	9479
2	browser	D-9	9728
2	browser	D-=	7680
2	browser	D-M-Left	9216
2	browser	D-M-Right	8960
2	browser	D-M-i	8448
2	browser	D-S-[	9216
2	browser	D-S-]	8960
2	browser	D-S-c	11008
2	browser	D-S-v	12544
2	browser	D-S-z	11520
2	browser	D-[	9984
2	browser	D-]	10240
2	browser	D-a	12800
2	browser	D-c	12032
2	browser	D-l	10752
2	browser	D-r	10496
2	browser	D-t	8704
2	browser	D-v	12288
2	browser	D-x	11776
2	browser	D-z	11264
2	sidebar	:	4096
2	sidebar	Down	4608
2	sidebar	End	5632
2	sidebar	Enter	3584
2	sidebar	Escape	3328
2	sidebar	G	5632
2	sidebar	Home	5376
2	sidebar	Left	4864
2	sidebar	Right	5120
2	sidebar	Up	4352
2	sidebar	g	5376
2	sidebar	h	4864
2	sidebar	j	4608
2	sidebar	k	4352
2	sidebar	l	5120
2	sidebar	q	3328
2	sidebar	r	3840
2	terminal	C-+	5888
2	terminal	C--	6144
2	terminal	C-=	5888
2	terminal	C-S-a	6912
2	terminal	C-S-c	6656
2	terminal	C-S-f	6400
2	terminal	C-S-v	7424
2	terminal	D-a	6912
2	terminal	D-c	6656
2	terminal	D-f	6400
2	terminal	D-v	7424
2	ui	D-+	2560
2	ui	D-,	2048
2	ui	D--	2816
2	ui	D-0	3072
2	ui	D-1	512
2	ui	D-2	513
2	ui	D-3	514
2	ui	D-4	515
2	ui	D-5	516
2	ui	D-6	517
2	ui	D-7	518
2	ui	D-8	519
2	ui	D-9	520
2	ui	D-=	2560
2	ui	D-S-d	1024
2	ui	D-S-n	256
2	ui	D-d	768
2	ui	D-k	2304
2	ui	D-n	0
2	ui	D-p	2304"""
