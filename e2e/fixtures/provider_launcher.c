/* A real provider-named process supervising the external Python protocol peer. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static volatile sig_atomic_t child_pid = -1;

static void forward_signal(int signal_number) {
    if (child_pid > 0) kill((pid_t)child_pid, signal_number);
}

int main(int argc, char **argv) {
    const char *name = strrchr(argv[0], '/');
    name = name ? name + 1 : argv[0];
    if (strcmp(name, "claude") && strcmp(name, "codex")) return 126;
    size_t length = strlen(JAM_FIXTURE_SCRIPTS) + strlen(name) + 2;
    char *script = malloc(length);
    char **arguments = calloc((size_t)argc + 2, sizeof(char *));
    if (!script || !arguments) return 127;
    snprintf(script, length, "%s/%s", JAM_FIXTURE_SCRIPTS, name);
    arguments[0] = JAM_FIXTURE_PYTHON;
    arguments[1] = script;
    for (int index = 1; index < argc; index++) arguments[index + 1] = argv[index];

    sigset_t blocked, previous;
    sigemptyset(&blocked);
    const int signals[] = {SIGTERM, SIGINT, SIGHUP, SIGQUIT};
    for (size_t index = 0; index < sizeof(signals) / sizeof(signals[0]); index++)
        sigaddset(&blocked, signals[index]);
    if (sigprocmask(SIG_BLOCK, &blocked, &previous)) return 127;
    child_pid = fork();
    if (child_pid < 0) return 127;
    if (child_pid == 0) {
        sigprocmask(SIG_SETMASK, &previous, NULL);
        execv(arguments[0], arguments);
        perror("provider protocol launch");
        _exit(127);
    }
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_handler = forward_signal;
    sigemptyset(&action.sa_mask);
    for (size_t index = 0; index < sizeof(signals) / sizeof(signals[0]); index++)
        if (sigaction(signals[index], &action, NULL)) return 127;
    sigprocmask(SIG_SETMASK, &previous, NULL);
    int status;
    while (waitpid((pid_t)child_pid, &status, 0) < 0) {
        if (errno != EINTR) return 127;
    }
    free(arguments);
    free(script);
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}
