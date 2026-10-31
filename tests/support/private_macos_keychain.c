/* Test-only throwaway identity; never open a default keychain or change lists.
 * Password/key bytes stay in RAM. stdout contains only an owned certificate hash.
 */
#include <Security/Security.h>
#include <CoreFoundation/CoreFoundation.h>
#include <CommonCrypto/CommonDigest.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <errno.h>
#include <limits.h>
#include <pwd.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char *fixture_cn = "Developer ID Application: Zerus mechanics fixture (FAKETEST01)";

static void checked(OSStatus status, const char *operation) {
    if (status != errSecSuccess) {
        fprintf(stderr, "mechanics (modelled identity boundary): %s status %d\n", operation, (int)status);
        exit(1);
    }
}

static void refuse(const char *message) {
    fprintf(stderr, "mechanics (modelled identity boundary): %s\n", message);
    exit(2);
}

static void private_path(const char *path, const char *leaf, int existing) {
    char home[PATH_MAX], owner_home[PATH_MAX], parent[PATH_MAX], resolved[PATH_MAX];
    const char *configured = getenv("HOME");
    struct passwd *owner = getpwuid(getuid());
    struct stat info;
    if (!configured || !realpath(configured, home) || !owner || !realpath(owner->pw_dir, owner_home)
        || getuid() != geteuid() || !getuid() || !strcmp(home, owner_home)
        || lstat(home, &info) || !S_ISDIR(info.st_mode)
        || info.st_uid != geteuid() || (info.st_mode & 077))
        refuse("an explicit private HOME is required");
    if (path[0] != '/' || strlen(path) >= sizeof(parent) || strstr(path, "/login.keychain")
        || !strcmp(path, "/Library/Keychains/System.keychain"))
        refuse("unsafe absolute private path");
    strcpy(parent, path);
    char *slash = strrchr(parent, '/');
    if (!slash || strcmp(slash + 1, leaf)) refuse("unexpected fixture leaf");
    *slash = 0;
    if (!realpath(parent, resolved) || strcmp(parent, resolved)
        || strncmp(parent, home, strlen(home)) || parent[strlen(home)] != '/'
        || lstat(parent, &info) || !S_ISDIR(info.st_mode) || info.st_uid != geteuid()
        || (info.st_mode & 077)) refuse("fixture parent is not private and canonical");
    if (existing) {
        if (lstat(path, &info) || !S_ISREG(info.st_mode) || info.st_uid != geteuid() || info.st_nlink != 1)
            refuse("fixture leaf is not an owned regular file");
    } else {
        if (!lstat(path, &info) || errno != ENOENT) refuse("fixture leaf is not a fresh absent path");
        if (access(parent, W_OK)) refuse("fixture parent is not writable");
    }
}

static void release(CFTypeRef value) { if (value) CFRelease(value); }

int main(int argc, char **argv) {
    struct rlimit no_core = {0, 0};
    if (setrlimit(RLIMIT_CORE, &no_core)) refuse("cannot disable core files");
    const char *enabled = getenv("ZERUS_TEST_PRIVATE_MACOS_SIGNING");
    if (!enabled || strcmp(enabled, "1") || (argc != 3 && argc != 4)) refuse("explicit fixture opt-in required");
    checked(SecKeychainSetUserInteractionAllowed(false), "disable interaction in fixture process");
    SecKeychainRef keychain = NULL;
    if (!strcmp(argv[1], "delete") && argc == 3) {
        private_path(argv[2], "fixture.keychain-db", 1);
        checked(SecKeychainOpen(argv[2], &keychain), "open explicit fixture keychain for deletion");
        if (!keychain) refuse("no explicit deletion reference");
        checked(SecKeychainDelete(keychain), "delete explicit fixture keychain");
        release(keychain);
        struct stat remaining;
        if (!lstat(argv[2], &remaining) || errno != ENOENT) refuse("fixture keychain deletion is unconfirmed");
        puts("{\"qualification\":\"mechanics (modelled identity boundary)\",\"keychain_deleted\":true}");
        return 0;
    }
    if (strcmp(argv[1], "create") || argc != 4) refuse("unknown fixture operation");
    private_path(argv[2], "fixture.keychain-db", 0);
    private_path(argv[3], "fixture.der", 0);
    const char *a = strrchr(argv[2], '/'), *b = strrchr(argv[3], '/');
    if ((a - argv[2]) != (b - argv[3]) || strncmp(argv[2], argv[3], (size_t)(a - argv[2])))
        refuse("keychain and certificate must share the private parent");

    /* A newly generated synthetic key/certificate arrives only on the pipe.
     * No plaintext private key or password is written to a file or argv. */
    unsigned char pem[65536];
    size_t bytes = fread(pem, 1, sizeof(pem), stdin);
    if (!bytes || bytes == sizeof(pem) || ferror(stdin)) refuse("invalid bounded fixture input");
    CFDataRef data = CFDataCreate(NULL, pem, (CFIndex)bytes);
    explicit_bzero(pem, sizeof(pem));
    if (!data) refuse("cannot allocate fixture input");
    unsigned char random[32];
    char fixture_password[65];
    checked(SecRandomCopyBytes(kSecRandomDefault, sizeof(random), random), "generate private password");
    for (size_t i = 0; i < sizeof(random); ++i) snprintf(fixture_password + i * 2, 3, "%02x", random[i]);
    explicit_bzero(random, sizeof(random));
    OSStatus created = SecKeychainCreate(argv[2], 64, fixture_password, false, NULL, &keychain);
    explicit_bzero(fixture_password, sizeof(fixture_password));
    checked(created, "create explicit fixture keychain without UI");
    if (!keychain) refuse("no explicit import reference");

    /* Access is limited at import to these two applications. No allow-all ACL,
     * partition-list command, default selector or later owner ACL change. */
    SecTrustedApplicationRef applications[2] = {NULL, NULL};
    checked(SecTrustedApplicationCreateFromPath("/usr/bin/codesign", &applications[0]), "codesign access object");
    checked(SecTrustedApplicationCreateFromPath(NULL, &applications[1]), "fixture self access object");
    if (!applications[0] || !applications[1]) refuse("explicit trusted applications missing");
    CFArrayRef trusted = CFArrayCreate(NULL, (const void **)applications, 2, &kCFTypeArrayCallBacks);
    if (!trusted) refuse("explicit trusted application array missing");
    SecAccessRef access = NULL;
    checked(SecAccessCreate(CFSTR("Zerus synthetic mechanics key"), trusted, &access), "private import access object");
    if (!access) refuse("explicit private import access missing");
    SecItemImportExportKeyParameters parameters = {0};
    parameters.version = SEC_KEY_IMPORT_EXPORT_PARAMS_VERSION;
    parameters.flags = kSecKeyImportOnlyOne;
    parameters.accessRef = access;
    SecExternalFormat format = kSecFormatPEMSequence;
    SecExternalItemType type = kSecItemTypeAggregate;
    CFArrayRef items = NULL;
    checked(SecItemImport(data, NULL, &format, &type, 0, &parameters, keychain, &items), "import only into private keychain");
    release(data);
    if (!items || CFArrayGetCount(items) > 3) refuse("unexpected fixture import contents");
    SecCertificateRef certificate = NULL;
    for (CFIndex i = 0; i < CFArrayGetCount(items); ++i) {
        CFTypeRef item = CFArrayGetValueAtIndex(items, i);
        if (CFGetTypeID(item) == SecCertificateGetTypeID()) {
            if (certificate) refuse("multiple fixture certificates");
            certificate = (SecCertificateRef)CFRetain(item);
        } else if (CFGetTypeID(item) == SecIdentityGetTypeID()) {
            if (certificate) refuse("multiple fixture certificates");
            checked(SecIdentityCopyCertificate((SecIdentityRef)item, &certificate), "fixture certificate");
        }
    }
    if (!certificate) refuse("fixture certificate missing");
    SecIdentityRef identity = NULL;
    checked(SecIdentityCreateWithCertificate(keychain, certificate, &identity), "prove private identity key/certificate pair");
    if (!identity) refuse("fixture identity missing");
    CFStringRef common_name = NULL;
    checked(SecCertificateCopyCommonName(certificate, &common_name), "fixture certificate name");
    CFStringRef expected = CFStringCreateWithCString(NULL, fixture_cn, kCFStringEncodingUTF8);
    if (!common_name || !CFEqual(common_name, expected)) refuse("unexpected fixture certificate name");
    CFDataRef der = SecCertificateCopyData(certificate);
    if (!der || CFDataGetLength(der) > 16384) refuse("unexpected fixture certificate size");
    int fd = open(argv[3], O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0600);
    if (fd < 0) refuse("cannot reserve public fixture certificate");
    ssize_t written = write(fd, CFDataGetBytePtr(der), (size_t)CFDataGetLength(der));
    if (written != CFDataGetLength(der) || fsync(fd) || close(fd)) refuse("public certificate write failed");
    unsigned char digest[CC_SHA1_DIGEST_LENGTH];
    CC_SHA1(CFDataGetBytePtr(der), (CC_LONG)CFDataGetLength(der), digest);
    printf("{\"qualification\":\"mechanics (modelled identity boundary)\",\"private_identity_proved\":true,\"sha1\":\"");
    for (size_t i = 0; i < sizeof(digest); ++i) printf("%02X", digest[i]);
    puts("\"}");
    release(der); release(common_name); release(expected); release(identity); release(certificate);
    release(items); release(access); release(trusted); release(applications[0]); release(applications[1]); release(keychain);
    return 0;
}
