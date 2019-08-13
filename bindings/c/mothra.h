#ifndef _MOTHRA_H_
#define _MOTHRA_H_

#ifdef __cplusplus
extern "C" {
#endif

extern void libp2p_start(char**, int length);
extern void libp2p_send_gossip(char*);

void receive_gossip(char*);


#ifdef __cplusplus
}
#endif

#endif // _MOTHRA_H_