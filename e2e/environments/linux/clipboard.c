/* Transfer fixture data on the owned Wayland desktop through ext-data-control. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
#include <wayland-client.h>

#include "ext-data-control.h"

#define MAX_TYPES 64
#define MAX_BYTES (32 * 1024 * 1024)

struct offer {
    struct ext_data_control_offer_v1 *handle;
    size_t count;
    char *types[MAX_TYPES];
};

struct clipboard {
    struct wl_seat *seat;
    struct ext_data_control_manager_v1 *manager;
    struct offer *selection;
    bool observed;
};

struct data {
    size_t count;
    char **types;
    int files[MAX_TYPES];
};

static void fail(const char *message) {
    fprintf(stderr, "%s\n", message);
    exit(1);
}

static int file_at(int directory, size_t index, int mode) {
    char name[24];
    snprintf(name, sizeof(name), "%zu", index);
    int fd = openat(directory, name, mode | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (fd < 0) fail("Could not open private clipboard payload");
    return fd;
}

static void ready(int fd, short events) {
    struct pollfd watch = {.fd = fd, .events = events};
    if (poll(&watch, 1, 6000) <= 0) fail("Wayland clipboard transfer timed out");
}

static size_t transfer(int from, int to, size_t limit) {
    char buffer[65536];
    size_t total = 0;
    for (;;) {
        ready(from, POLLIN);
        ssize_t length = read(from, buffer, sizeof(buffer));
        if (length < 0) fail("Could not read Wayland clipboard data");
        if (length == 0) return total;
        total += (size_t)length;
        if (total > limit) fail("Wayland clipboard snapshot exceeds its byte limit");
        for (ssize_t offset = 0; offset < length;) {
            ready(to, POLLOUT);
            ssize_t written = write(to, buffer + offset, (size_t)(length - offset));
            if (written < 0 && errno == EPIPE) return total;
            if (written < 0 && errno == EAGAIN) continue;
            if (written <= 0) fail("Could not write Wayland clipboard data");
            offset += written;
        }
    }
}

static bool content_type(const char *mime) {
    /* X11 selection-management targets are requests, not clipboard data. */
    return strcmp(mime, "TARGETS") && strcmp(mime, "MULTIPLE") &&
           strcmp(mime, "TIMESTAMP") && strcmp(mime, "SAVE_TARGETS");
}

static void offered(void *data, struct ext_data_control_offer_v1 *handle, const char *mime) {
    (void)handle;
    struct offer *offer = data;
    if (!content_type(mime)) return;
    if (offer->count >= MAX_TYPES || strpbrk(mime, "\r\n"))
        fail("Invalid Wayland clipboard format list");
    offer->types[offer->count] = strdup(mime);
    if (!offer->types[offer->count++]) fail("Could not record offered clipboard format");
}

static const struct ext_data_control_offer_v1_listener offer_listener = {.offer = offered};

static void discard_offer(struct offer *offer) {
    if (!offer) return;
    ext_data_control_offer_v1_destroy(offer->handle);
    for (size_t i = 0; i < offer->count; ++i) free(offer->types[i]);
    free(offer);
}

static void new_offer(void *data, struct ext_data_control_device_v1 *device,
                      struct ext_data_control_offer_v1 *handle) {
    (void)data;
    (void)device;
    struct offer *offer = calloc(1, sizeof(*offer));
    if (!offer) fail("Could not record Wayland clipboard offer");
    offer->handle = handle;
    ext_data_control_offer_v1_add_listener(handle, &offer_listener, offer);
}

static void selected(void *data, struct ext_data_control_device_v1 *device,
                     struct ext_data_control_offer_v1 *handle) {
    (void)device;
    struct clipboard *clipboard = data;
    discard_offer(clipboard->selection);
    clipboard->selection = handle ? ext_data_control_offer_v1_get_user_data(handle) : NULL;
    clipboard->observed = true;
}

static void primary(void *data, struct ext_data_control_device_v1 *device,
                    struct ext_data_control_offer_v1 *handle) {
    (void)data;
    (void)device;
    if (handle) discard_offer(ext_data_control_offer_v1_get_user_data(handle));
}

static void finished(void *data, struct ext_data_control_device_v1 *device) {
    (void)data;
    (void)device;
    fail("The compositor disconnected its clipboard device");
}

static const struct ext_data_control_device_v1_listener device_listener = {
    .data_offer = new_offer,
    .selection = selected,
    .finished = finished,
    .primary_selection = primary,
};

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    (void)version;
    struct clipboard *clipboard = data;
    if (!strcmp(interface, wl_seat_interface.name)) {
        if (clipboard->seat) fail("The isolated compositor needs exactly one seat");
        clipboard->seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
    } else if (!strcmp(interface, ext_data_control_manager_v1_interface.name)) {
        clipboard->manager = wl_registry_bind(registry, name, &ext_data_control_manager_v1_interface, 1);
    }
}

static void removed(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {.global = global, .global_remove = removed};

static void send_data(void *data, struct ext_data_control_source_v1 *source,
                      const char *mime, int fd) {
    (void)source;
    const struct data *payload = data;
    int flags = fcntl(fd, F_GETFL);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0)
        fail("Could not bound the Wayland clipboard transfer");
    for (size_t i = 0; i < payload->count; ++i) {
        if (strcmp(payload->types[i], mime)) continue;
        if (lseek(payload->files[i], 0, SEEK_SET) < 0) fail("Could not seek private clipboard data");
        transfer(payload->files[i], fd, MAX_BYTES);
        break;
    }
    close(fd);
}

static void cancelled(void *data, struct ext_data_control_source_v1 *source) {
    (void)data;
    ext_data_control_source_v1_destroy(source);
    exit(0);
}

static const struct ext_data_control_source_v1_listener source_listener = {
    .send = send_data,
    .cancelled = cancelled,
};

static void capture(struct wl_display *display, const struct offer *offer, int directory) {
    if (!offer) return;
    size_t total = 0;
    for (size_t i = 0; i < offer->count; ++i) {
        fprintf(stderr, "Reading native clipboard type %s\n", offer->types[i]);
        int pipe_fds[2];
        if (pipe2(pipe_fds, O_CLOEXEC) != 0) fail("Could not create clipboard transport pipe");
        ext_data_control_offer_v1_receive(offer->handle, offer->types[i], pipe_fds[1]);
        close(pipe_fds[1]);
        if (wl_display_flush(display) < 0) fail("Could not request Wayland clipboard representation");
        int output = file_at(directory, i, O_WRONLY | O_CREAT | O_EXCL);
        total += transfer(pipe_fds[0], output, MAX_BYTES - total);
        close(output);
        close(pipe_fds[0]);
        puts(offer->types[i]);
    }
}

int main(int argc, char **argv) {
    if (argc < 3 || (strcmp(argv[1], "capture") && strcmp(argv[1], "restore")))
        fail("Usage: jam-e2e-clipboard capture|restore PRIVATE_DIRECTORY [FORMAT ...]");
    const char *expected = getenv("KWIN_PID");
    const char *nested = getenv("JAM_E2E_NESTED_X11");
    if (!expected || !nested || strcmp(nested, "1") || !getenv("WAYLAND_DISPLAY"))
        fail("Clipboard access requires the isolated KWin test desktop");
    signal(SIGPIPE, SIG_IGN);
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) fail("Could not open the private Wayland display");
    struct ucred credentials;
    socklen_t credentials_size = sizeof(credentials);
    if (getsockopt(wl_display_get_fd(display), SOL_SOCKET, SO_PEERCRED,
                   &credentials, &credentials_size) != 0 ||
        credentials.pid != strtol(expected, NULL, 10) || credentials.uid != geteuid())
        fail("Wayland connection does not belong to the owned KWin process");

    struct clipboard clipboard = {0};
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, &clipboard);
    if (wl_display_roundtrip(display) < 0 || !clipboard.seat || !clipboard.manager)
        fail("The owned KWin session does not provide native multi-format clipboard control");
    struct ext_data_control_device_v1 *device =
        ext_data_control_manager_v1_get_data_device(clipboard.manager, clipboard.seat);
    ext_data_control_device_v1_add_listener(device, &device_listener, &clipboard);
    if (wl_display_roundtrip(display) < 0 || !clipboard.observed)
        fail("The owned KWin session did not report its clipboard");

    int directory = open(argv[2], O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (directory < 0) fail("Could not open private clipboard directory");
    if (!strcmp(argv[1], "capture")) {
        capture(display, clipboard.selection, directory);
        return 0;
    }

    struct data payload = {.count = (size_t)(argc - 3), .types = argv + 3};
    if (payload.count > MAX_TYPES) fail("Too many Wayland clipboard formats");
    struct ext_data_control_source_v1 *source = NULL;
    if (payload.count) {
        source = ext_data_control_manager_v1_create_data_source(clipboard.manager);
        ext_data_control_source_v1_add_listener(source, &source_listener, &payload);
        for (size_t i = 0; i < payload.count; ++i) {
            payload.files[i] = file_at(directory, i, O_RDONLY);
            ext_data_control_source_v1_offer(source, payload.types[i]);
        }
    }
    close(directory);
    ext_data_control_device_v1_set_selection(device, source);
    if (wl_display_roundtrip(display) < 0) fail("The compositor rejected the clipboard");
    puts("ready");
    fflush(stdout);
    if (source) {
        while (wl_display_dispatch(display) >= 0) {}
        fail("The owned Wayland display closed unexpectedly");
    }
    return 0;
}
