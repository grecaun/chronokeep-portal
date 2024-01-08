#! /bin/bash

if [[ "$EUID" -eq 0 ]]; then
    echo "This script is not meant to be run as root. Please run as a user with sudo privileges."
    exit 1
fi;
sudo -k
if ! sudo true; then
    echo "This script requires sudo privileges to run."
    exit 1
fi;
sudo ping -c 1 1.1.1.1 > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    echo "This script requires an internet connection to work."
    exit 1
fi;
echo "------------------------------------------------"
echo "--------- Now installing Chronoportal! ---------"
echo "------------------------------------------------"
export USER=$(whoami)
echo "---------------- User is $USER -----------------"
echo "------------------------------------------------"
git --version > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    echo "---- Installing git. ----"
    sudo apt install git -y
fi;
if ! [[ -e /home/$USER/portal ]]; then
    echo "------------- Cloning portal repo --------------"
    echo "------------------------------------------------"
    git clone https://github.com/grecaun/chronokeep-portal.git /home/$USER/portal
fi;
if ! [[ -e /home/$USER/portal-quit ]]; then
    echo "----------- Cloning portal quit repo -----------"
    echo "------------------------------------------------"
    git clone https://github.com/grecaun/chronokeep-portal-quit.git /home/$USER/portal-quit
fi;
echo "------- Setting git directories as safe. -------"
echo "------------------------------------------------"
git config --global --add safe.direcotry /home/$USER/portal
git config --global --add safe.direcotry /home/$USER/portal-quit
if ! [[ -e /portal/ ]]; then
    echo "---------- Creating portal directory. ----------"
    echo "------------------------------------------------"
    sudo mkdir /portal/
    sudo chown $USER:root /portal/
fi;
if ! [[ -e /portal/run.sh ]]; then
    echo "--------- Creating portal run script. ----------"
    echo "------------------------------------------------"
    echo "#!/bin/bash" > /portal/run.sh
    echo >> /portal/run.sh
    echo "export PORTAL_UPDATE_SCRIPT=\"/portal/update_portal.sh\"" >> /portal/run.sh
    echo "export PORTAL_DATABASE_PATH=\"/portal/chronokeep-portal.sqlite\"" >> /portal/run.sh
    echo "/portal/chronokeep-portal >> /portal/portal.log 2>> /portal/portal.log" >> /portal/run.sh
    sudo chown $USER:root /portal/run.sh
    sudo chmod +x /portal/run.sh
fi;
if ! [[ -e /portal/quit.sh ]]; then
    echo "--------- Creating portal quit script. ---------"
    echo "------------------------------------------------"
    echo "#!/bin/bash" > /portal/quit.sh
    echo >> /portal/quit.sh
    echo "/portal/chronokeep-portal-quit >> /portal/quit.log" >> /portal/quit.sh
    sudo chown $USER:root /portal/quit.sh
    sudo chmod +x /portal/quit.sh
fi;
if ! [[ -e /portal/update_portal.sh ]]; then
    echo "----------- Creating update script. ------------"
    echo "------------------------------------------------"
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
    sudo chown $USER:root /portal/update_portal.sh
    sudo chmod +x /portal/update_portal.sh
fi;
source "$HOME/.cargo/env"
rustup -V > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    curl -V
    if [[ $? != 0 ]]; then
        echo "--------------- Installing curl. ---------------"
        echo "------------------------------------------------"
        sudo apt install curl -y
    fi;
    echo "--------------- Installing rust. ---------------"
    echo "------------------------------------------------"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi;
sudo apt list --installed 2>> /dev/null | grep libssl-dev > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    echo "------- Installing libssl-dev (OpenSSL). -------"
    echo "------------------------------------------------"
    sudo apt install libssl-dev -y
fi;
sudo apt list --installed 2>> /dev/null | grep pkg-config > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    echo "------------ Installing pkg-config. ------------"
    echo "------------------------------------------------"
    sudo apt install pkg-config -y
fi;
cc -v > /dev/null 2> /dev/null
if [[ $? != 0 ]]; then
    echo "--------------- Installing gcc. ----------------"
    echo "------------------------------------------------"
    sudo apt install gcc -y
fi;
if ! [[ -e /etc/systemd/system/portal.service ]]; then
    echo "----------- Creating portal service. -----------"
    echo "------------------------------------------------"
    sudo echo "    [Unit]" | sudo tee /etc/systemd/system/portal.service
    sudo echo "Description=Chronokeep Portal Service" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "Wants=network-online.target" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "After=network.target network-online.target" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "StartLimitIntervalSec=0" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "[Service]" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "Type=simple" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "Restart=on-failure" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "RestartSec=1" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "User=$USER" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "ExecStart=/portal/run.sh" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "[Install]" | sudo tee -a /etc/systemd/system/portal.service
    sudo echo "WantedBy=multi-user.target" | sudo tee -a /etc/systemd/system/portal.service
fi;
if ! [[ -e /etc/systemd/system/portal-quit.service ]]; then
    echo "-------- Creating portal quit service. ---------"
    echo "------------------------------------------------"
    sudo echo "[Unit]" | sudo tee /etc/systemd/system/portal-quit.service
    sudo echo "Description=Ensure Chronokeep Portal closes before a server shutdown occurs." | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo "DefaultDependencies=no" | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo "Before=shutdown.target" | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo  | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo "[Service]" | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo "Type=oneshot" | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo "ExecStart=/portal/quit.sh" | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo "TimeoutStartSec=0" | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo  | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo "[Install]" | sudo tee -a /etc/systemd/system/portal-quit.service
    sudo echo "WantedBy=shutdown.target" | sudo tee -a /etc/systemd/system/portal-quit.service
fi;
echo "--------- Reloading systemctl daemons. ---------"
echo "------------------------------------------------"
sudo systemctl daemon-reload
echo "----------- Enabling portal service ------------"
echo "------------------------------------------------"
sudo systemctl enable portal
echo "----------- Starting portal service. -----------"
echo "------------------------------------------------"
sudo systemctl start portal
if ! [[ -e /etc/sudoers.d/chronoportal ]]; then
    echo "----------- Setting up nopasswd sudo -----------"
    echo "------------------------------------------------"
    if [[ -e /etc/sudoers.d/010_pi-nopasswd ]]; then
        sudo rm /etc/sudoers.d/010_pi-nopasswd
    fi;
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/bin/date" | sudo tee /etc/sudoers.d/chronoportal
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/reboot" | sudo tee -a /etc/sudoers.d/chronoportal
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/shutdown" | sudo tee -a /etc/sudoers.d/chronoportal
fi;
echo "---------- Running the update script. ----------"
echo "------------------------------------------------"
/portal/update_portal.sh
echo "-------------- Setup is finished! --------------"
echo "------------------------------------------------"