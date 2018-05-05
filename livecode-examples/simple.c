// Simple C livecode example

#include <unistd.h>
#include <math.h>

// CAREFUL: make sure these match the input or bad things will happen
const size_t BUFSIZE = 1024;
const size_t CHANNELS = 2;

// processing function: absolute value distortion
void f(float frame[CHANNELS]) {
    for (size_t i = 0; i < CHANNELS; i++)
        frame[i] = fabs(frame[i])*2-1;
}

int main() {
    float buffer[BUFSIZE][CHANNELS];
    for(;;) {
        read(STDIN_FILENO, buffer, sizeof buffer);
        for (size_t i = 0; i < BUFSIZE; i++)
            f(buffer[i]);
        write(STDOUT_FILENO, buffer, sizeof buffer);
    }
}
