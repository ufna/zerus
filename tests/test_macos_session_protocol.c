/* Local protocol validation only. Does not join sessions or access Keychain. */
#include "../src/macos_session.c"
#include <assert.h>
#include <stdio.h>

static void sender(struct packet *packet, uid_t euid, uid_t ruid) {
    mach_msg_audit_trailer_t *audit = (void *)((char *)packet + packet->header.msgh_size);
    memset(audit, 0, sizeof(*audit));
    audit->msgh_trailer_type = MACH_MSG_TRAILER_FORMAT_0;
    audit->msgh_trailer_size = sizeof(*audit);
    audit->msgh_audit.val[1] = euid;
    audit->msgh_audit.val[3] = ruid;
}

int main(void) {
    assert(getuid() != 0 && getuid() == geteuid());
    struct packet request = {0};
    request.header.msgh_size = sizeof(mach_msg_header_t);
    request.header.msgh_id = REQUEST_ID;
    request.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_PORT_SEND_ONCE, 0);
    request.header.msgh_remote_port = 123;
    sender(&request, geteuid(), getuid());
    assert(valid_request(&request));
    struct packet wrong = request; wrong.header.msgh_id++; assert(!valid_request(&wrong));
    wrong = request; wrong.header.msgh_bits |= MACH_MSGH_BITS_COMPLEX; assert(!valid_request(&wrong));
    wrong = request; wrong.header.msgh_remote_port = MACH_PORT_NULL; assert(!valid_request(&wrong));
    wrong = request; wrong.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_PORT_SEND, 0); assert(!valid_request(&wrong));
    wrong = request; sender(&wrong, geteuid()+1, getuid()); assert(!valid_request(&wrong));
    wrong = request; sender(&wrong, geteuid(), getuid()+1); assert(!valid_request(&wrong));
    wrong = request; wrong.header.msgh_size = 0; assert(!valid_request(&wrong));
    wrong = request; wrong.header.msgh_size = sizeof(wrong)+1; assert(!valid_request(&wrong));
    wrong = request; memset((char *)&wrong+sizeof(mach_msg_header_t),0,sizeof(mach_msg_audit_trailer_t)); assert(!valid_request(&wrong));

    struct packet response = {0};
    response.header.msgh_size = REPLY_SIZE;
    response.header.msgh_id = RESPONSE_ID;
    response.header.msgh_bits = MACH_MSGH_BITS_COMPLEX;
    response.body.msgh_descriptor_count = 1;
    response.session.name = 123;
    response.session.disposition = MACH_MSG_TYPE_PORT_SEND;
    response.session.type = MACH_MSG_PORT_DESCRIPTOR;
    sender(&response, geteuid(), getuid());
    assert(valid_response(&response));
    wrong = response; sender(&wrong, geteuid()+1, getuid()); assert(!valid_response(&wrong));
    wrong = response; wrong.header.msgh_id++; assert(!valid_response(&wrong));
    wrong = response; wrong.body.msgh_descriptor_count = 2; assert(!valid_response(&wrong));
    wrong = response; wrong.session.disposition = MACH_MSG_TYPE_PORT_RECEIVE; assert(!valid_response(&wrong));
    wrong = response; wrong.session.type = MACH_MSG_OOL_DESCRIPTOR; assert(!valid_response(&wrong));
    wrong = response; wrong.session.name = MACH_PORT_DEAD; assert(!valid_response(&wrong));
    wrong = response; wrong.header.msgh_bits = 0; assert(!valid_response(&wrong));
    puts("macOS session protocol: valid peers accepted, malformed and foreign-user packets rejected");
}
