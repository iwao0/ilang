// Build a 1_000_000-node singly-linked list, traverse summing values,
// then free. The sum is printed so the optimiser can't elide.
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct Node {
    int64_t value;
    struct Node *next;
} Node;

#define N 1000000

int main(void) {
    Node *head = NULL;
    for (int64_t i = 0; i < N; i++) {
        Node *n = malloc(sizeof(Node));
        n->value = i;
        n->next = head;
        head = n;
    }
    int64_t sum = 0;
    for (Node *p = head; p; p = p->next) sum += p->value;
    Node *p = head;
    while (p) {
        Node *next = p->next;
        free(p);
        p = next;
    }
    printf("%lld\n", (long long)sum);
    return 0;
}
