#ifndef SC16_NET_H
#define SC16_NET_H

#define SC16_NET_NAME_CAPACITY 16
#define SC16_NET_MAX_PAYLOAD 1024

int sc16_net_discover(char *names, unsigned int capacity);
int sc16_net_send(const char *recipient, const void *payload,
                  unsigned int length);
int sc16_net_receive(char *sender, void *payload, unsigned int capacity);

#endif
