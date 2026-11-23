#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wayland-client.h>
#include "screencast-client.h"

static struct zkde_screencast_unstable_v1 *manager;

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version)
{
    if (strcmp(interface, zkde_screencast_unstable_v1_interface.name) == 0 && version >= 3)
        manager = wl_registry_bind(registry, name, &zkde_screencast_unstable_v1_interface, 3);
}

static void removed(void *data, struct wl_registry *registry, uint32_t name) {}
static void closed(void *data, struct zkde_screencast_stream_unstable_v1 *stream) { exit(0); }
static void created(void *data, struct zkde_screencast_stream_unstable_v1 *stream, uint32_t node)
{
    printf("%u\n", node);
    fflush(stdout);
}
static void failed(void *data, struct zkde_screencast_stream_unstable_v1 *stream, const char *error)
{
    fprintf(stderr, "%s\n", error);
    exit(1);
}

int main(int argc, char **argv)
{
    if (argc != 5)
        return 2;
    struct wl_display *display = wl_display_connect(NULL);
    if (!display)
        return 1;
    struct wl_registry *registry = wl_display_get_registry(display);
    const struct wl_registry_listener globals = {global, removed};
    wl_registry_add_listener(registry, &globals, NULL);
    if (wl_display_roundtrip(display) < 0 || !manager)
        return 1;
    struct zkde_screencast_stream_unstable_v1 *stream =
        zkde_screencast_unstable_v1_stream_region(manager, atoi(argv[1]), atoi(argv[2]),
            atoi(argv[3]), atoi(argv[4]), wl_fixed_from_int(1),
            ZKDE_SCREENCAST_UNSTABLE_V1_POINTER_HIDDEN);
    const struct zkde_screencast_stream_unstable_v1_listener events = {
        .closed = closed, .created = created, .failed = failed
    };
    zkde_screencast_stream_unstable_v1_add_listener(stream, &events, NULL);
    while (wl_display_dispatch(display) >= 0) {}
    return 1;
}
