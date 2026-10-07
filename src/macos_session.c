/* Join this user's existing desktop security session before spawning Claude.
 * No credentials or passwords pass through this service. Mach audit trailers
 * authenticate both peers; only the same non-root Unix user may join.
 */
#include <Security/Security.h>
#include <bsm/audit.h>
#include <bsm/libbsm.h>
#include <mach/mach.h>
#include <servers/bootstrap.h>
#include <string.h>
#include <unistd.h>

#define SERVICE "com.hgdev.hgs.user-session"
#define REQUEST_ID 0x48475301
#define RESPONSE_ID 0x48475302

struct packet {
    mach_msg_header_t header;
    mach_msg_body_t body;
    mach_msg_port_descriptor_t session;
    unsigned char trailer[sizeof(mach_msg_audit_trailer_t) + 4];
};
#define REPLY_SIZE (sizeof(mach_msg_header_t) + sizeof(mach_msg_body_t) + sizeof(mach_msg_port_descriptor_t))

static mach_msg_audit_trailer_t *peer(struct packet *packet) {
    size_t size = packet->header.msgh_size;
    if (size < sizeof(mach_msg_header_t) || size > REPLY_SIZE) return NULL;
    size = (size + 3) & ~(size_t)3;
    if (size + sizeof(mach_msg_audit_trailer_t) > sizeof(*packet)) return NULL;
    mach_msg_audit_trailer_t *trailer = (void *)((unsigned char *)packet + size);
    if (trailer->msgh_trailer_type != MACH_MSG_TRAILER_FORMAT_0 ||
        trailer->msgh_trailer_size < sizeof(*trailer) || geteuid() == 0 ||
        audit_token_to_euid(trailer->msgh_audit) != geteuid() ||
        audit_token_to_ruid(trailer->msgh_audit) != getuid()) return NULL;
    return trailer;
}

static int valid_request(struct packet *packet) {
    return peer(packet) && packet->header.msgh_id == REQUEST_ID &&
        packet->header.msgh_size == sizeof(mach_msg_header_t) &&
        !(packet->header.msgh_bits & MACH_MSGH_BITS_COMPLEX) &&
        MACH_MSGH_BITS_REMOTE(packet->header.msgh_bits) == MACH_MSG_TYPE_PORT_SEND_ONCE &&
        MACH_PORT_VALID(packet->header.msgh_remote_port);
}

static int valid_response(struct packet *packet) {
    return peer(packet) && packet->header.msgh_id == RESPONSE_ID &&
        packet->header.msgh_size == REPLY_SIZE &&
        (packet->header.msgh_bits & MACH_MSGH_BITS_COMPLEX) &&
        packet->body.msgh_descriptor_count == 1 &&
        packet->session.type == MACH_MSG_PORT_DESCRIPTOR &&
        packet->session.disposition == MACH_MSG_TYPE_PORT_SEND &&
        MACH_PORT_VALID(packet->session.name);
}

int hgs_macos_session_graphical(void) {
    SecuritySessionId session;
    SessionAttributeBits flags;
    return SessionGetInfo(callerSecuritySession, &session, &flags) == errSecSuccess &&
        (flags & sessionHasGraphicAccess) != 0;
}

int hgs_macos_keychain_state(void) {
    SecKeychainRef keychain = NULL;
    SecKeychainStatus status = 0;
    if (SecKeychainCopyDefault(&keychain) != errSecSuccess) return 0;
    OSStatus result = SecKeychainGetStatus(keychain, &status);
    CFRelease(keychain);
    return result != errSecSuccess ? 0 : (status & kSecUnlockStateStatus) ? 2 : 1;
}

int hgs_macos_session_join(void) {
    if (hgs_macos_session_graphical()) return 0;
    if (getuid() != geteuid() || geteuid() == 0) return 1;
    mach_port_t service = MACH_PORT_NULL, reply = MACH_PORT_NULL;
    if (bootstrap_look_up(bootstrap_port, SERVICE, &service) != KERN_SUCCESS) return 1;
    if (mach_port_allocate(mach_task_self(), MACH_PORT_RIGHT_RECEIVE, &reply) != KERN_SUCCESS) {
        mach_port_deallocate(mach_task_self(), service);
        return 1;
    }
    struct packet message = {0};
    message.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_COPY_SEND, MACH_MSG_TYPE_MAKE_SEND_ONCE);
    message.header.msgh_size = sizeof(mach_msg_header_t);
    message.header.msgh_remote_port = service;
    message.header.msgh_local_port = reply;
    message.header.msgh_id = REQUEST_ID;
    kern_return_t result = mach_msg(&message.header,
        MACH_SEND_MSG | MACH_RCV_MSG | MACH_SEND_TIMEOUT | MACH_RCV_TIMEOUT |
        MACH_RCV_TRAILER_TYPE(MACH_MSG_TRAILER_FORMAT_0) | MACH_RCV_TRAILER_ELEMENTS(MACH_RCV_TRAILER_AUDIT),
        message.header.msgh_size, sizeof(message), reply, 2000, MACH_PORT_NULL);
    int failed = 1;
    if (result == KERN_SUCCESS) {
        mach_msg_audit_trailer_t *sender = peer(&message);
        if (valid_response(&message)) {
            mach_port_t previous = audit_session_self();
            if (MACH_PORT_VALID(previous)) {
                au_asid_t joined = audit_session_join(message.session.name);
                if (joined != (au_asid_t)-1 && joined == audit_token_to_asid(sender->msgh_audit) &&
                    hgs_macos_session_graphical()) failed = 0;
                else (void)audit_session_join(previous);
                mach_port_deallocate(mach_task_self(), previous);
            }
        }
        mach_msg_destroy(&message.header);
    }
    mach_port_destroy(mach_task_self(), reply);
    mach_port_deallocate(mach_task_self(), service);
    return failed;
}

int hgs_macos_session_serve(void) {
    if (getuid() != geteuid() || geteuid() == 0 || !hgs_macos_session_graphical()) return 1;
    mach_port_t service = MACH_PORT_NULL;
    if (bootstrap_check_in(bootstrap_port, SERVICE, &service) != KERN_SUCCESS) return 1;
    for (;;) {
        struct packet request = {0};
        kern_return_t result = mach_msg(&request.header,
            MACH_RCV_MSG | MACH_RCV_TIMEOUT |
            MACH_RCV_TRAILER_TYPE(MACH_MSG_TRAILER_FORMAT_0) | MACH_RCV_TRAILER_ELEMENTS(MACH_RCV_TRAILER_AUDIT),
            0, sizeof(request), service, 60000, MACH_PORT_NULL);
        if (result == MACH_RCV_TIMED_OUT) break; /* launchd starts us again on demand */
        if (result != KERN_SUCCESS) break;
        if (!valid_request(&request)) {
            mach_msg_destroy(&request.header);
            continue;
        }
        struct packet response = {0};
        response.header.msgh_bits = MACH_MSGH_BITS(MACH_MSG_TYPE_MOVE_SEND_ONCE, 0) | MACH_MSGH_BITS_COMPLEX;
        response.header.msgh_size = REPLY_SIZE;
        response.header.msgh_remote_port = request.header.msgh_remote_port;
        response.header.msgh_id = RESPONSE_ID;
        response.body.msgh_descriptor_count = 1;
        response.session.name = audit_session_self();
        response.session.disposition = MACH_MSG_TYPE_MOVE_SEND;
        response.session.type = MACH_MSG_PORT_DESCRIPTOR;
        if (!MACH_PORT_VALID(response.session.name)) {
            mach_msg_destroy(&request.header);
            continue;
        }
        result = mach_msg(&response.header, MACH_SEND_MSG | MACH_SEND_TIMEOUT,
            response.header.msgh_size, 0, MACH_PORT_NULL, 1000, MACH_PORT_NULL);
        if (result != KERN_SUCCESS) mach_msg_destroy(&response.header);
    }
    mach_port_destroy(mach_task_self(), service);
    return 0;
}
