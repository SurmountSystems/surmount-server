# GeneratedRegistryRaceTest.Load / upb-test: terminate called without an active exception.
# Upstream cmake has no exclude flag for that test. The library build stays.
# protobuf_36 is the mail-host protobuf 36.2. Do not read prev.protobuf:
# that name is an alias of protobuf_36, and forcing it recurses.
final: prev:
let
  protobufNoCheck = prev.protobuf_36.overrideAttrs (_old: {
    doCheck = false;
  });
in
{
  protobuf_36 = protobufNoCheck;
  protobuf = protobufNoCheck;
}
