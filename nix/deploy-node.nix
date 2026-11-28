# One deploy-rs node for the mail host.
# Activation is deploy-rs activate.nixos. This file does not copy that script.
# No addresses, tokens, or PEM material.
{ self, deploy-rs }:
{
  nodes.mail-vps = {
    hostname = "surmount-1";
    sshUser = "root";
    sshOpts = [ "-o" "IdentityFile=~/.ssh/id_ed25519" "-o" "BatchMode=yes" "-o" "IdentitiesOnly=yes" ];
    autoRollback = true;
    magicRollback = true;
    profiles.system = {
      user = "root";
      autoRollback = true;
      magicRollback = true;
      path = deploy-rs.lib.x86_64-linux.activate.nixos self.nixosConfigurations.mail-vps;
    };
  };
}
