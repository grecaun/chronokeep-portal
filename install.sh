#! /bin/bash

sudo -k
if ! sudo true; then
    echo "This script requires sudo privileges to run."
    exit 1
fi;
if [[ "$EUID" -eq 0 ]]; then
    echo "This script is not meant to be run as root. Please run as a user with sudo privileges."
    exit 1
fi;
echo "--------------------------------------"
echo "---- Now installing Chronoportal! ----"
echo "--------------------------------------"
export USER=$(whoami)
echo "---- User is $USER ----"
echo "---- Installing git. ----"
sudo apt install git -y
export PORTALNUM=$((1 + $RANDOM % 100))
if [[ $1 -gt 0 ]]; then
    export PORTALNUM=$1
fi;
if [[ -e /home/$USER/portal ]] || [[ -e /home/$USER/portal-quit ]]; then
    echo "---- One or more directories already exist. ----"
else
    echo "---- Clone github repos. ----"
    git clone git@github.com:grecaun/chronokeep-portal /home/$USER/portal
    git clone git@github.com:grecaun/chronokeep-portal-quit /home/$USER/portal-quit
fi;
echo "---- Setting git directories as safe. ----"
git config --global --add safe.direcotry /home/$USER/portal
git config --global --add safe.direcotry /home/$USER/portal-quit
if [[ -e /portal/ ]]; then
    echo "Portal directory already exists."
else
    echo "---- Creating portal directory. ----"
    sudo mkdir /portal/
    sudo chown $USER:root /portal/
fi;
if [[ -e /portal/run.sh ]]; then
    echo "---- Portal run script already exists. ----"
else
    echo "---- Creating portal run script. ----"
    echo "#!/bin/bash" > /portal/run.sh
    echo >> /portal/run.sh
    echo "export PORTAL_UPDATE_SCRIPT=\"/portal/update_portal.sh\"" >> /portal/run.sh
    echo "/portal/chronokeep-portal >> /portal/portal.log 2>> /portal/portal.log" >> /portal/run.sh
    sudo chown $USER:root /portal/run.sh
    sudo chmod +x /portal/run.sh
fi;
if [[ -e /portal/quit.sh ]]; then
    echo "---- Portal quit script already exists. ----"
else
    echo "---- Creating portal quit script. ----"
    echo "#!/bin/bash" > /portal/quit.sh
    echo >> /portal/quit.sh
    echo "/portal/chronokeep-portal-quit >> /portal/quit.log" >> /portal/quit.sh
    sudo chown $USER:root /portal/run.sh
    sudo chmod +x /portal/run.sh
fi;
if [[ -e /portal/update_portal.sh ]]; then
    echo "---- Update script already exists. ----"
else
    echo "---- Creating update script. ----"
    echo "#!/bin/bash" > /portal/update_portal.sh
    echo >> /portal/update_portal.sh
    echo "echo Pulling git for newest version or portal." >> /portal/update_portal.sh
    echo "git -C /home/$USER/portal pull" >> /portal/update_portal.sh
    echo "echo Building portal software." >> /portal/update_portal.sh
    echo "cd /home/$USER/portal && /home/$USER/.cargo/bin/cargo build --release" >> /portal/update_portal.sh
    echo "echo Moving portal software to run location." >> /portal/update_portal.sh
    echo "sudo mv -f ./target/release/chronokeep-portal /portal/" >> /portal/update_portal.sh
    echo "echo Pulling git for newest version of portal-quit." >> /portal/update_portal.sh
    echo "git -C /home/$USER/portal-quit pull" >> /portal/update_portal.sh
    echo "echo Building portal-quit software." >> /portal/update_portal.sh
    echo "cd /home/$USER/portal-quit && /home/$USER/.cargo/bin/cargo build --release" >> /portal/update_portal.sh
    echo "echo Moving portal-quit software to run location." >> /portal/update_portal.sh
    echo "sudo mv -f ./target/release/chronokeep-portal-quit /portal/" >> /portal/update_portal.sh
    echo "echo Updating ownership of /portal" >> /portal/update_portal.sh
    echo "sudo chown -R $USER:root /portal" >> /portal/update_portal.sh
    echo "echo Restarting portal software." >> /portal/update_portal.sh
    echo "sudo systemctl restart portal" >> /portal/update_portal.sh
fi;
rustup -V > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    echo "Installing rust."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi;
echo "---- Installing libssl-dev (OpenSSL). ----"
sudo apt install libssl-dev -y
if [[ -e /etc/systemd/system/portal.service ]]; then
    echo "---- Portal service already exists. ----"
else
    echo "---- Creating portal service. ----"
    echo "    [Unit]" > /etc/systemd/system/portal.service
    echo "Description=Chronokeep Portal Service" >> /etc/systemd/system/portal.service
    echo "Wants=network-online.target" >> /etc/systemd/system/portal.service
    echo "After=network.target network-online.target" >> /etc/systemd/system/portal.service
    echo "StartLimitIntervalSec=0" >> /etc/systemd/system/portal.service
    echo >> /etc/systemd/system/portal.service
    echo "[Service]" >> /etc/systemd/system/portal.service
    echo "Type=simple" >> /etc/systemd/system/portal.service
    echo "Restart=on-failure" >> /etc/systemd/system/portal.service
    echo "RestartSec=1" >> /etc/systemd/system/portal.service
    echo "User=$USER" >> /etc/systemd/system/portal.service
    echo "ExecStart=/portal/run.sh" >> /etc/systemd/system/portal.service
    echo >> /etc/systemd/system/portal.service
    echo "[Install]" >> /etc/systemd/system/portal.service
    echo "WantedBy=multi-user.target" >> /etc/systemd/system/portal.service
fi;
if [[ -e /etc/systemd/system/portal-quit.service ]]; then
    echo "---- Portal quit service already exists. ----"
else
    echo "---- Creating portal quit service. ----"
    echo "[Unit]" > /etc/systemd/system/portal-quit.service
    echo "Description=Ensure Chronokeep Portal closes before a server shutdown occurs." >> /etc/systemd/system/portal-quit.service
    echo "DefaultDependencies=no" >> /etc/systemd/system/portal-quit.service
    echo "Before=shutdown.target" >> /etc/systemd/system/portal-quit.service
    echo  >> /etc/systemd/system/portal-quit.service
    echo "[Service]" >> /etc/systemd/system/portal-quit.service
    echo "Type=oneshot" >> /etc/systemd/system/portal-quit.service
    echo "ExecStart=/portal/quit.sh" >> /etc/systemd/system/portal-quit.service
    echo "TimeoutStartSec=0" >> /etc/systemd/system/portal-quit.service
    echo  >> /etc/systemd/system/portal-quit.service
    echo "[Install]" >> /etc/systemd/system/portal-quit.service
    echo "WantedBy=shutdown.target" >> /etc/systemd/system/portal-quit.service
fi;
echo "---- Reloading systemctl daemons, enabling portal service, and starting portal service. ----"
sudo systemctl daemon-reload
sudo systemctl enable portal
sudo systemctl start portal
if [[ -e /etc/sudoers.d/chronoportal ]]; then
    echo "---- User already set up for nopasswd for reboot, shutdown, and date functions. ----"
else
    if [[ -e /etc/sudoers.d/010_pi-nopasswd ]]; then
        sudo rm /etc/sudoers.d/010_pi-nopasswd
    fi;
    echo "$USER ALL=(ALL) NOPASSWD: /usr/bin/date" > /etc/sudoers.d/chronoportal
    echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/reboot" >> /etc/sudoers.d/chronoportal
    echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/shutdown" >> /etc/sudoers.d/chronoportal
fi;
/portal/update_porta.sh
echo "----------------------------"
echo "---- Setup is finished! ----"
echo "----------------------------"