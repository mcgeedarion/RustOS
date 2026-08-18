/*
 * userspace/wayland/input.c - Input handling for the compositor
 *
 * Implements:
 *   1. Input device initialization from /dev/input/event*
 *   2. evdev event parsing
 *   3. wl_pointer event routing (enter, leave, motion, button, axis)
 *   4. wl_keyboard.enter/leave on focus change
 *   5. wl_touch support
 */

#include "compositor_types.h"
#include <linux/input.h>
#include <linux/input-event-codes.h>
#include <dirent.h>
#include <time.h>

#ifndef BITS_PER_LONG
#define BITS_PER_LONG 64
#endif

/* Helper macro for testing bits in evdev bit arrays */
#define test_bit(bit, array) \
    ((array[(bit) / BITS_PER_LONG] >> ((bit) % BITS_PER_LONG)) & 1)

/* ── Input state ─────────────────────────────────────────────────────────── */
typedef struct {
    int fd;
    int type;              /* EV_REL or EV_ABS */
    int has_touch;
    int has_mouse;
    int has_keyboard;
} InputDevice;

extern InputDevice input_devices[8];
extern int n_input_devices;

/* Pointer state - exported for window manager access */
PointerState pointer_state_public = {
    .x = 0,
    .y = 0,
    .buttons = 0,
    .serial = 0,
    .focus_surface = NULL,
    .focus_client = NULL,
    .entered = 0,
};

/* Keyboard focus state */
static struct {
    Surface *focus_surface;
    Client *focus_client;
    uint32_t modifiers;
} keyboard_state = {
    .focus_surface = NULL,
    .focus_client = NULL,
    .modifiers = 0,
};

/* Touch state */
static struct {
    int32_t x, y;
    int slot;
    uint32_t serial;
    Surface *focus_surface;
    Client *focus_client;
} touch_state = {
    .x = 0,
    .y = 0,
    .slot = 0,
    .serial = 0,
    .focus_surface = NULL,
    .focus_client = NULL,
};

/* ── Helper functions ────────────────────────────────────────────────────── */
static uint32_t next_pointer_serial(void) {
    pointer_state_public.serial++;
    if (pointer_state_public.serial == 0) pointer_state_public.serial = 1;
    return pointer_state_public.serial;
}

static uint32_t next_keyboard_serial(void) {
    static uint32_t serial = 0;
    serial++;
    if (serial == 0) serial = 1;
    return serial;
}

static uint32_t next_touch_serial(void) {
    touch_state.serial++;
    if (touch_state.serial == 0) touch_state.serial = 1;
    return touch_state.serial;
}

/* Find surface at given coordinates */
static Surface *find_surface_at(int32_t x, int32_t y, Client **out_client) {
    Surface *result = NULL;
    Client *result_client = NULL;
    
    /* Check layer surfaces first (top to bottom for hit testing) */
    for (int li = ZWL_LAYER_OVERLAY; li >= ZWL_LAYER_BACKGROUND; li--) {
        for (int ci = 0; ci < g.n_clients; ci++) {
            Client *c = &g.clients[ci];
            if (!c->alive) continue;
            for (int i = 0; i < MAX_LAYER_SURFACES; i++) {
                LayerSurface *ls = &c->layer_surfaces[i];
                if (!ls->id || !ls->configured || (int)ls->layer != li) continue;
                if (x >= ls->x && x < ls->x + ls->w &&
                    y >= ls->y && y < ls->y + ls->h) {
                    Surface *s = find_surface(c, ls->surface_id);
                    if (s && s->committed) {
                        result = s;
                        result_client = c;
                        goto found;
                    }
                }
            }
        }
    }
    
    /* Check regular surfaces (simple last-wins for MVP) */
    for (int ci = 0; ci < g.n_clients; ci++) {
        Client *c = &g.clients[ci];
        if (!c->alive) continue;
        for (int si = 0; si < MAX_SURFACES; si++) {
            Surface *s = &c->surfaces[si];
            if (!s->id || !s->committed || s->parent_surface_id) continue;
            if (s->role == SURFACE_ROLE_LAYER || s->role == SURFACE_ROLE_SUBSURFACE) continue;
            
            WlBuffer *wb = find_buffer(c, s->attached_buffer_id);
            if (!wb) continue;
            
            int32_t sx = s->x;
            int32_t sy = s->y;
            int32_t sw = wb->width;
            int32_t sh = wb->height;
            
            /* Include SSD in hit test for non-CSD windows */
            XdgToplevel *xt = find_xdg_toplevel_for_surface(c, s);
            if (xt && !xt->has_csd) {
                sx -= SSD_BORDER_W;
                sy -= SSD_TITLEBAR_H + SSD_BORDER_W;
                sw += SSD_BORDER_W * 2;
                sh += SSD_TITLEBAR_H + SSD_BORDER_W * 2;
            }
            
            if (x >= sx && x < sx + sw && y >= sy && y < sy + sh) {
                result = s;
                result_client = c;
            }
        }
    }
    
found:
    if (out_client) *out_client = result_client;
    return result;
}

/* ── Pointer event helpers ───────────────────────────────────────────────── */
static void send_pointer_enter(Client *c, Surface *s) {
    if (!c || !s || !c->pointer_id) return;
    
    uint32_t serial = next_pointer_serial();
    uint8_t payload[32];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    memcpy(payload + sz, &s->id, 4); sz += 4;
    
    /* Send surface coordinates as fixed-point */
    int32_t sx_fp = wl_fixed_from_int(pointer_state_public.x - s->x);
    int32_t sy_fp = wl_fixed_from_int(pointer_state_public.y - s->y);
    memcpy(payload + sz, &sx_fp, 4); sz += 4;
    memcpy(payload + sz, &sy_fp, 4); sz += 4;
    
    wl_send(c->fd, c->pointer_id, WL_POINTER_EVT_ENTER, payload, (uint16_t)sz);
    pointer_state_public.entered = 1;
    pointer_state_public.focus_surface = s;
    pointer_state_public.focus_client = c;
}

static void send_pointer_leave(Client *c, Surface *s) {
    if (!c || !s || !c->pointer_id) return;
    
    uint32_t serial = next_pointer_serial();
    uint8_t payload[8];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    memcpy(payload + sz, &s->id, 4); sz += 4;
    
    wl_send(c->fd, c->pointer_id, WL_POINTER_EVT_LEAVE, payload, (uint16_t)sz);
    pointer_state_public.entered = 0;
    pointer_state_public.focus_surface = NULL;
    pointer_state_public.focus_client = NULL;
}

static void send_pointer_motion(uint32_t time_ms) {
    Client *c = pointer_state_public.focus_client;
    Surface *s = pointer_state_public.focus_surface;
    if (!c || !s || !c->pointer_id) return;
    
    uint8_t payload[20];
    size_t sz = 0;
    
    uint32_t timestamp = time_ms;
    int32_t sx_fp = wl_fixed_from_int(pointer_state_public.x - s->x);
    int32_t sy_fp = wl_fixed_from_int(pointer_state_public.y - s->y);
    
    memcpy(payload + sz, &timestamp, 4); sz += 4;
    memcpy(payload + sz, &sx_fp, 4); sz += 4;
    memcpy(payload + sz, &sy_fp, 4); sz += 4;
    
    wl_send(c->fd, c->pointer_id, WL_POINTER_EVT_MOTION, payload, (uint16_t)sz);
}

static void send_pointer_button(uint32_t time_ms, uint32_t button, uint32_t state) {
    Client *c = pointer_state_public.focus_client;
    if (!c || !c->pointer_id) return;
    
    uint32_t serial = next_pointer_serial();
    uint8_t payload[20];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    uint32_t timestamp = time_ms;
    memcpy(payload + sz, &timestamp, 4); sz += 4;
    memcpy(payload + sz, &button, 4); sz += 4;
    memcpy(payload + sz, &state, 4); sz += 4;
    
    wl_send(c->fd, c->pointer_id, WL_POINTER_EVT_BUTTON, payload, (uint16_t)sz);
    
    /* Update button mask */
    if (state == 1) {
        pointer_state_public.buttons |= (1u << button);
    } else {
        pointer_state_public.buttons &= ~(1u << button);
    }
}

static void send_pointer_axis(uint32_t time_ms, uint32_t axis, int32_t value) {
    Client *c = pointer_state_public.focus_client;
    if (!c || !c->pointer_id) return;
    
    uint8_t payload[20];
    size_t sz = 0;
    
    uint32_t timestamp = time_ms;
    int32_t v = wl_fixed_from_int(value);
    
    memcpy(payload + sz, &timestamp, 4); sz += 4;
    memcpy(payload + sz, &axis, 4); sz += 4;
    memcpy(payload + sz, &v, 4); sz += 4;
    
    wl_send(c->fd, c->pointer_id, WL_POINTER_EVT_AXIS, payload, (uint16_t)sz);
}

/* ── Keyboard event helpers ──────────────────────────────────────────────── */
static void send_keyboard_enter(Client *c, Surface *s) {
    if (!c || !s || !c->keyboard_id) return;
    
    uint32_t serial = next_keyboard_serial();
    uint8_t payload[16];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    memcpy(payload + sz, &s->id, 4); sz += 4;
    
    /* Send empty key array for enter */
    uint32_t keys_len = 0;
    memcpy(payload + sz, &keys_len, 4); sz += 4;
    memcpy(payload + sz, &keys_len, 4); sz += 4;  /* padding */
    
    wl_send(c->fd, c->keyboard_id, WL_KEYBOARD_EVT_ENTER, payload, (uint16_t)sz);
    keyboard_state.focus_surface = s;
    keyboard_state.focus_client = c;
}

static void send_keyboard_leave(Client *c, Surface *s) {
    if (!c || !s || !c->keyboard_id) return;
    
    uint32_t serial = next_keyboard_serial();
    uint8_t payload[8];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    memcpy(payload + sz, &s->id, 4); sz += 4;
    
    wl_send(c->fd, c->keyboard_id, WL_KEYBOARD_EVT_LEAVE, payload, (uint16_t)sz);
    keyboard_state.focus_surface = NULL;
    keyboard_state.focus_client = NULL;
}

static void send_keyboard_key(uint32_t time_ms, uint32_t key, uint32_t state) {
    Client *c = keyboard_state.focus_client;
    if (!c || !c->keyboard_id) return;
    
    uint32_t serial = next_keyboard_serial();
    uint8_t payload[16];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    uint32_t timestamp = time_ms;
    memcpy(payload + sz, &timestamp, 4); sz += 4;
    memcpy(payload + sz, &key, 4); sz += 4;
    memcpy(payload + sz, &state, 4); sz += 4;
    
    wl_send(c->fd, c->keyboard_id, WL_KEYBOARD_EVT_KEY, payload, (uint16_t)sz);
}

static void send_keyboard_modifiers(uint32_t mods_depressed, uint32_t mods_latched,
                                     uint32_t mods_locked, uint32_t group) {
    Client *c = keyboard_state.focus_client;
    if (!c || !c->keyboard_id) return;
    
    uint32_t serial = next_keyboard_serial();
    uint8_t payload[20];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    memcpy(payload + sz, &mods_depressed, 4); sz += 4;
    memcpy(payload + sz, &mods_latched, 4); sz += 4;
    memcpy(payload + sz, &mods_locked, 4); sz += 4;
    memcpy(payload + sz, &group, 4); sz += 4;
    
    wl_send(c->fd, c->keyboard_id, WL_KEYBOARD_EVT_MODIFIERS, payload, (uint16_t)sz);
}

/* ── Touch event helpers ─────────────────────────────────────────────────── */
static void send_touch_down(uint32_t time_ms, int32_t id, int32_t x, int32_t y) {
    Client *c = touch_state.focus_client;
    Surface *s = touch_state.focus_surface;
    if (!c || !s || !c->seat_id) return;
    
    /* Get touch pointer object if created */
    uint32_t touch_id = 0;
    for (int ci = 0; ci < g.n_clients; ci++) {
        if (&g.clients[ci] == c) {
            touch_id = g.clients[ci].pointer_id;  /* TODO: separate touch object */
            break;
        }
    }
    if (!touch_id) return;
    
    uint32_t serial = next_touch_serial();
    uint8_t payload[28];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    uint32_t timestamp = time_ms;
    memcpy(payload + sz, &timestamp, 4); sz += 4;
    memcpy(payload + sz, &s->id, 4); sz += 4;
    memcpy(payload + sz, (uint32_t*)&id, 4); sz += 4;
    
    int32_t sx_fp = wl_fixed_from_int(x - s->x);
    int32_t sy_fp = wl_fixed_from_int(y - s->y);
    memcpy(payload + sz, &sx_fp, 4); sz += 4;
    memcpy(payload + sz, &sy_fp, 4); sz += 4;
    
    wl_send(c->fd, touch_id, WL_TOUCH_EVT_DOWN, payload, (uint16_t)sz);
}

static void send_touch_up(uint32_t time_ms, int32_t id) {
    Client *c = touch_state.focus_client;
    if (!c || !c->seat_id) return;
    
    uint32_t touch_id = 0;
    for (int ci = 0; ci < g.n_clients; ci++) {
        if (&g.clients[ci] == c) {
            touch_id = g.clients[ci].pointer_id;
            break;
        }
    }
    if (!touch_id) return;
    
    uint32_t serial = next_touch_serial();
    uint8_t payload[16];
    size_t sz = 0;
    
    memcpy(payload + sz, &serial, 4); sz += 4;
    uint32_t timestamp = time_ms;
    memcpy(payload + sz, &timestamp, 4); sz += 4;
    memcpy(payload + sz, (uint32_t*)&id, 4); sz += 4;
    
    wl_send(c->fd, touch_id, WL_TOUCH_EVT_UP, payload, (uint16_t)sz);
}

static void send_touch_motion(uint32_t time_ms, int32_t id, int32_t x, int32_t y) {
    Client *c = touch_state.focus_client;
    Surface *s = touch_state.focus_surface;
    if (!c || !s || !c->seat_id) return;
    
    uint32_t touch_id = 0;
    for (int ci = 0; ci < g.n_clients; ci++) {
        if (&g.clients[ci] == c) {
            touch_id = g.clients[ci].pointer_id;
            break;
        }
    }
    if (!touch_id) return;
    
    uint8_t payload[24];
    size_t sz = 0;
    
    uint32_t timestamp = time_ms;
    memcpy(payload + sz, &timestamp, 4); sz += 4;
    memcpy(payload + sz, (uint32_t*)&id, 4); sz += 4;
    
    int32_t sx_fp = wl_fixed_from_int(x - s->x);
    int32_t sy_fp = wl_fixed_from_int(y - s->y);
    memcpy(payload + sz, &sx_fp, 4); sz += 4;
    memcpy(payload + sz, &sy_fp, 4); sz += 4;
    
    wl_send(c->fd, touch_id, WL_TOUCH_EVT_MOTION, payload, (uint16_t)sz);
}

/* ── Focus management ────────────────────────────────────────────────────── */
static void update_pointer_focus(void) {
    Client *new_client = NULL;
    Surface *new_surface = find_surface_at(pointer_state_public.x, pointer_state_public.y, &new_client);
    
    /* Handle leave from old surface */
    if (pointer_state_public.focus_surface && pointer_state_public.focus_surface != new_surface) {
        send_pointer_leave(pointer_state_public.focus_client, pointer_state_public.focus_surface);
    }
    
    /* Handle enter to new surface */
    if (new_surface && new_surface != pointer_state_public.focus_surface) {
        send_pointer_enter(new_client, new_surface);
    }
}

static void update_keyboard_focus(void) {
    /* For now, keyboard follows the focused client's topmost surface */
    if (g.focused_client < 0 || g.focused_client >= g.n_clients) {
        if (keyboard_state.focus_surface) {
            send_keyboard_leave(keyboard_state.focus_client, keyboard_state.focus_surface);
        }
        return;
    }
    
    Client *c = &g.clients[g.focused_client];
    if (!c->alive) return;
    
    /* Find topmost committed surface for this client */
    Surface *top_surface = NULL;
    for (int si = 0; si < MAX_SURFACES; si++) {
        Surface *s = &c->surfaces[si];
        if (s->id && s->committed && !s->parent_surface_id &&
            s->role != SURFACE_ROLE_SUBSURFACE) {
            top_surface = s;
        }
    }
    
    if (top_surface && top_surface != keyboard_state.focus_surface) {
        if (keyboard_state.focus_surface) {
            send_keyboard_leave(keyboard_state.focus_client, keyboard_state.focus_surface);
        }
        send_keyboard_enter(c, top_surface);
    } else if (!top_surface && keyboard_state.focus_surface) {
        send_keyboard_leave(keyboard_state.focus_client, keyboard_state.focus_surface);
    }
}

/* ── Input device initialization ─────────────────────────────────────────── */
static int open_input_device(const char *path) {
    if (n_input_devices >= (int)(sizeof(input_devices) / sizeof(input_devices[0]))) {
        return -1;
    }
    
    int fd = open(path, O_RDONLY | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0) return -1;
    
    /* Query device capabilities */
    unsigned long evbits[(EV_CNT + BITS_PER_LONG - 1) / BITS_PER_LONG] = {0};
    unsigned long keybits[(KEY_CNT + BITS_PER_LONG - 1) / BITS_PER_LONG] = {0};
    
    if (ioctl(fd, EVIOCGBIT(0, sizeof(evbits)), evbits) < 0) {
        close(fd);
        return -1;
    }
    
    InputDevice *dev = &input_devices[n_input_devices];
    memset(dev, 0, sizeof(*dev));
    dev->fd = fd;
    
    /* Check for relevant event types */
    if (test_bit(EV_KEY, evbits)) {
        if (ioctl(fd, EVIOCGBIT(EV_KEY, sizeof(keybits)), keybits) >= 0) {
            if (test_bit(KEY_ENTER, keybits) || test_bit(KEY_SPACE, keybits)) {
                dev->has_keyboard = 1;
            }
        }
    }
    
    if (test_bit(EV_REL, evbits) && test_bit(REL_X, evbits) && test_bit(REL_Y, evbits)) {
        dev->has_mouse = 1;
        dev->type = EV_REL;
    }
    
    if (test_bit(EV_ABS, evbits) && test_bit(ABS_X, evbits) && test_bit(ABS_Y, evbits)) {
        if (test_bit(BTN_TOUCH, keybits)) {
            dev->has_touch = 1;
        }
        if (!dev->has_mouse) {
            dev->type = EV_ABS;
        }
    }
    
    if (dev->has_keyboard || dev->has_mouse || dev->has_touch) {
        n_input_devices++;
        return fd;
    }
    
    close(fd);
    return -1;
}

static void scan_input_devices(void) {
    const char *base_path = "/dev/input";
    DIR *dir = opendir(base_path);
    if (!dir) return;
    
    struct dirent *ent;
    while ((ent = readdir(dir)) != NULL) {
        if (strncmp(ent->d_name, "event", 5) == 0) {
            char path[256];
            snprintf(path, sizeof(path), "%s/%s", base_path, ent->d_name);
            open_input_device(path);
        }
    }
    
    closedir(dir);
}

/* ── Event processing ────────────────────────────────────────────────────── */
static uint32_t get_time_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

static void process_evdev_event(struct input_event *ev) {
    uint32_t time_ms = get_time_ms();
    
    switch (ev->type) {
    case EV_REL:
        if (ev->code == REL_X) {
            pointer_state_public.x += ev->value;
            if (pointer_state_public.x < 0) pointer_state_public.x = 0;
            if (pointer_state_public.x >= (int32_t)g.screen_width) 
                pointer_state_public.x = (int32_t)g.screen_width - 1;
            update_pointer_focus();
            send_pointer_motion(time_ms);
        } else if (ev->code == REL_Y) {
            pointer_state_public.y += ev->value;
            if (pointer_state_public.y < 0) pointer_state_public.y = 0;
            if (pointer_state_public.y >= (int32_t)g.screen_height)
                pointer_state_public.y = (int32_t)g.screen_height - 1;
            update_pointer_focus();
            send_pointer_motion(time_ms);
        } else if (ev->code == REL_WHEEL) {
            send_pointer_axis(time_ms, WL_POINTER_AXIS_VERTICAL_SCROLL, -ev->value);
        } else if (ev->code == REL_HWHEEL) {
            send_pointer_axis(time_ms, WL_POINTER_AXIS_HORIZONTAL_SCROLL, -ev->value);
        }
        break;
        
    case EV_ABS:
        if (ev->code == ABS_X) {
            if (touch_state.focus_surface) {
                touch_state.x = ev->value;
                send_touch_motion(time_ms, touch_state.slot, touch_state.x, touch_state.y);
            } else {
                pointer_state_public.x = ev->value;
                update_pointer_focus();
                send_pointer_motion(time_ms);
            }
        } else if (ev->code == ABS_Y) {
            if (touch_state.focus_surface) {
                touch_state.y = ev->value;
                send_touch_motion(time_ms, touch_state.slot, touch_state.x, touch_state.y);
            } else {
                pointer_state_public.y = ev->value;
                update_pointer_focus();
                send_pointer_motion(time_ms);
            }
        }
        break;
        
    case EV_KEY:
        if (ev->code >= BTN_MOUSE && ev->code <= BTN_TASK) {
            uint32_t button = ev->code - BTN_MOUSE + 1;
            uint32_t state = (uint32_t)ev->value;
            send_pointer_button(time_ms, button, state);
        } else if (ev->code == BTN_TOUCH) {
            if (ev->value == 1) {
                /* Find surface under touch */
                Client *c = NULL;
                Surface *s = find_surface_at(touch_state.x, touch_state.y, &c);
                if (s && c) {
                    touch_state.focus_surface = s;
                    touch_state.focus_client = c;
                    send_touch_down(time_ms, touch_state.slot, touch_state.x, touch_state.y);
                }
            } else {
                if (touch_state.focus_surface) {
                    send_touch_up(time_ms, touch_state.slot);
                    touch_state.focus_surface = NULL;
                    touch_state.focus_client = NULL;
                }
            }
        } else if (ev->code < KEY_CNT && ev->value <= 1) {
            /* Keyboard key */
            uint32_t key = ev->code;
            uint32_t state = (uint32_t)ev->value;
            send_keyboard_key(time_ms, key, state);
            
            /* Track modifiers */
            uint32_t mod_mask = keyboard_state.modifiers;
            switch (ev->code) {
            case KEY_LEFTSHIFT:
            case KEY_RIGHTSHIFT:
                if (state) mod_mask |= (1u << 0);
                else mod_mask &= ~(1u << 0);
                break;
            case KEY_LEFTCTRL:
            case KEY_RIGHTCTRL:
                if (state) mod_mask |= (1u << 2);
                else mod_mask &= ~(1u << 2);
                break;
            case KEY_LEFTALT:
            case KEY_RIGHTALT:
                if (state) mod_mask |= (1u << 3);
                else mod_mask &= ~(1u << 3);
                break;
            }
            if (mod_mask != keyboard_state.modifiers) {
                keyboard_state.modifiers = mod_mask;
                send_keyboard_modifiers(mod_mask, 0, 0, 0);
            }
        }
        break;
        
    case EV_SYN:
        /* Synchronization event - usually ignored in simple implementations */
        break;
    }
}

static void handle_input_fd(int fd) {
    struct input_event ev;
    ssize_t n;
    
    while ((n = read(fd, &ev, sizeof(ev))) == sizeof(ev)) {
        process_evdev_event(&ev);
    }
}

/* ── Public API ──────────────────────────────────────────────────────────── */
int input_init(void) {
    n_input_devices = 0;
    memset(input_devices, 0, sizeof(input_devices));
    scan_input_devices();
    
    if (n_input_devices == 0) {
        return -1;
    }
    
    /* Add epoll fds for all input devices */
    for (int i = 0; i < n_input_devices; i++) {
        struct epoll_event epev;
        memset(&epev, 0, sizeof(epev));
        epev.events = EPOLLIN | EPOLLPRI;
        epev.data.fd = input_devices[i].fd;
        if (epoll_ctl(g.epoll_fd, EPOLL_CTL_ADD, input_devices[i].fd, &epev) < 0) {
            /* Continue anyway - some devices may fail */
        }
    }
    
    /* Update seat capabilities to include touch if available */
    uint32_t caps = WL_SEAT_CAP_POINTER | WL_SEAT_CAP_KEYBOARD;
    for (int i = 0; i < n_input_devices; i++) {
        if (input_devices[i].has_touch) {
            caps |= WL_SEAT_CAP_TOUCH;
            break;
        }
    }
    
    /* Notify all clients of updated capabilities */
    for (int ci = 0; ci < g.n_clients; ci++) {
        Client *c = &g.clients[ci];
        if (c->alive && c->seat_id) {
            wl_send(c->fd, c->seat_id, WL_SEAT_EVT_CAPABILITIES, &caps, 4);
        }
    }
    
    return 0;
}

void input_handle_event(int fd) {
    for (int i = 0; i < n_input_devices; i++) {
        if (input_devices[i].fd == fd) {
            handle_input_fd(fd);
            break;
        }
    }
}

void input_set_focused_client(int client_idx) {
    if (client_idx >= 0 && client_idx < g.n_clients) {
        g.focused_client = client_idx;
        update_keyboard_focus();
    }
}
