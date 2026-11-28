/* Measure changes in the displayed scene after X11 pointer input. */
#define _POSIX_C_SOURCE 200809L
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/extensions/XTest.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static double now_ms(void)
{
    struct timespec time;
    clock_gettime(CLOCK_MONOTONIC, &time);
    return time.tv_sec * 1000.0 + time.tv_nsec / 1000000.0;
}

static uint64_t read_scene(Display *display, Window window, int width, int height)
{
    XImage *image = XGetImage(display, window, 0, 0, width, height, AllPlanes, ZPixmap);
    if (!image) {
        fputs("could not read the X11 window\n", stderr);
        exit(1);
    }
    uint64_t hash = UINT64_C(0xcbf29ce484222325);
    for (int y = 2; y < height; y += 5) {
        for (int x = 2; x < width; x += 5) {
            hash ^= XGetPixel(image, x, y);
            hash *= UINT64_C(0x100000001b3);
        }
    }
    XDestroyImage(image);
    return hash;
}

static void click(Display *display, Window window, int x, int y)
{
    Window child;
    int root_x, root_y;
    if (!XTranslateCoordinates(display, window, DefaultRootWindow(display),
                               x, y, &root_x, &root_y, &child)) {
        fputs("could not locate the X11 control\n", stderr);
        exit(1);
    }
    XTestFakeMotionEvent(display, DefaultScreen(display), root_x, root_y, 0);
    XTestFakeButtonEvent(display, 1, True, 0);
    XTestFakeButtonEvent(display, 1, False, 0);
    XFlush(display);
}

int main(int argc, char **argv)
{
    if (argc != 6 && argc != 8) {
        fputs("usage: motion-x11 WINDOW WIDTH HEIGHT CLICK_X DURATION_MS [SECOND_X SECOND_DELAY_MS]\n", stderr);
        return 2;
    }
    Display *display = XOpenDisplay(NULL);
    if (!display) {
        fputs("could not open the X11 display\n", stderr);
        return 1;
    }
    Window window = strtoul(argv[1], NULL, 0);
    int width = atoi(argv[2]), height = atoi(argv[3]), button_x = atoi(argv[4]);
    int duration = atoi(argv[5]), second_x = argc == 8 ? atoi(argv[6]) : 0;
    int second_delay = argc == 8 ? atoi(argv[7]) : duration;
    uint64_t previous = read_scene(display, window, width, height);
    printf("0 %016llx\n", (unsigned long long)previous);
    double start = now_ms();
    click(display, window, button_x, height + 24);

    double last_read = start, largest_gap = 0, total_gap = 0;
    unsigned reads = 0;
    while (now_ms() - start < duration) {
        if (now_ms() - start >= second_delay) {
            click(display, window, second_x, height + 24);
            second_delay = duration;
        }
        uint64_t hash = read_scene(display, window, width, height);
        double captured = now_ms();
        double gap = captured - last_read;
        largest_gap = gap > largest_gap ? gap : largest_gap;
        total_gap += gap;
        last_read = captured;
        reads++;
        if (hash != previous) {
            printf("%.3f %016llx\n", captured - start, (unsigned long long)hash);
            previous = hash;
        }
        nanosleep(&(struct timespec){ .tv_nsec = 4000000 }, NULL);
    }
    printf("poll %.3f %.3f\n", total_gap / reads, largest_gap);
    XCloseDisplay(display);
    return 0;
}
