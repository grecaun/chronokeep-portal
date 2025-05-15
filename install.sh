#! /bin/bash

PORTAL_DEST=/portal/
SERVICE_NAME=portal
QUIT_SERVICE_NAME=portal-quit
UPDATE_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/update.sh'
UNINSTALL_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/uninstall.sh'
PORTAL_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal/releases/latest'
QUIT_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal-quit/releases/latest'

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
# Check OS type & architecture
OS=$(uname)
if [[ $? -ne 0 ]] || [[ $OS != "Linux" ]]; then
    echo "This script is only designed to work on linux."
    exit 1
fi;
ARCH=$(uname -m)
if [[ $ARCH != "x86_64" ]] && [[ $ARCH != armv7* ]] && [[ $ARCH != aarch64* ]]; then
    echo "This script does not recognize the system's architecture."
    exit 1
fi;
if [[ $ARCH == "x86_64" ]]; then
    TARGET=amd64-linux
else
    if [[ $ARCH == aarch64* ]]; then
        TARGET=aarch64-linux
    else
        TARGET=armv7-linux
    fi;
fi;
echo $TARGET
echo "------------------------------------------------"
echo "--------- Now installing Chronoportal! ---------"
echo "------------------------------------------------"
echo "-------- Checking for required packages --------"
echo "------------------------------------------------"
curl -V > /dev/null 2>&1
if [[ $? != 0 ]]; then
    echo "--------------- Installing curl. ---------------"
    echo "------------------------------------------------"
    sudo apt install curl -y
fi;
sudo apt list --installed 2>> /dev/null | grep alsa-utils > /dev/null 2>&1
if [[ $? != 0 ]]; then
    echo "------------ Installing alsa-utils. ------------"
    echo "------------------------------------------------"
    sudo apt install alsa-utils -y
fi;
echo | ts > /dev/null 2>&1
if [[ $? != 0 ]]; then
    echo "------------- Installing moreutils -------------"
    echo "------------------------------------------------"
    sudo apt install moreutils -y
fi;
export USER=$(whoami)
if ! [[ -e ${PORTAL_DEST} ]]; then
    echo "---------- Creating portal directory. ----------"
    echo "------------------------------------------------"
    sudo mkdir ${PORTAL_DEST}
fi;
if ! [[ -e ${PORTAL_DEST}logs/ ]]; then
    echo "----------- Creating logs directory. -----------"
    echo "------------------------------------------------"
    sudo mkdir ${PORTAL_DEST}logs/
fi;
sudo chown -R $USER:root ${PORTAL_DEST}
if ! [[ -e ${PORTAL_DEST}run.sh ]]; then
    echo "--------- Creating portal run script. ----------"
    echo "------------------------------------------------"
    echo "#!/bin/bash" | sudo tee ${PORTAL_DEST}run.sh
    echo | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_UPDATE_SCRIPT=\"${PORTAL_DEST}update.sh\"" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_DATABASE_PATH=\"${PORTAL_DEST}chronokeep-portal.sqlite\"" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_SCREEN_BUS=1" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_LEFT_BUTTON=11" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_UP_BUTTON=5" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_DOWN_BUTTON=6" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_RIGHT_BUTTON=13" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_ENTER_BUTTON=26" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "export PORTAL_ZEBRA_SHIFT=True" | sudo tee -a ${PORTAL_DEST}run.sh
    echo | sudo tee -a ${PORTAL_DEST}run.sh
    echo "now=\`date +%Y-%m-%d\`" | sudo tee -a ${PORTAL_DEST}run.sh
    echo "${PORTAL_DEST}chronokeep-portal | ts '[%Y-%m-%d %H:%M:%S]' >> ${PORTAL_DEST}logs/\${now}-portal.log 2>&1" | sudo tee -a ${PORTAL_DEST}run.sh
    sudo chown $USER:root ${PORTAL_DEST}run.sh
    sudo chmod +x ${PORTAL_DEST}run.sh
fi;
if ! [[ -e ${PORTAL_DEST}quit.sh ]]; then
    echo "--------- Creating portal quit script. ---------"
    echo "------------------------------------------------"
    echo "#!/bin/bash" | sudo tee ${PORTAL_DEST}quit.sh
    echo | sudo tee -a ${PORTAL_DEST}quit.sh
    echo "${PORTAL_DEST}chronokeep-portal-quit >> ${PORTAL_DEST}quit.log" | sudo tee -a ${PORTAL_DEST}quit.sh
    sudo chown $USER:root ${PORTAL_DEST}quit.sh
    sudo chmod +x ${PORTAL_DEST}quit.sh
fi;
echo "------------ Checking update script ------------"
echo "------------------------------------------------"
if ! [[ -e ${PORTAL_DEST}update.sh ]]; then
    echo "----------- Fetching update script. ------------"
    echo "------------------------------------------------"
    curl -L ${UPDATE_SCRIPT_URL} -o ${PORTAL_DEST}update.sh > /dev/null 2>&1
    sudo chown $USER:root ${PORTAL_DEST}update.sh
    sudo chmod +x ${PORTAL_DEST}update.sh
else
    OLD_SCRIPT_VERS=`cat ${PORTAL_DEST}update.sh | grep ^VERSION= | sed 's/VERSION=//'`
    if [[ $OLD_SCRIPT_VERS -ge 0 ]]; then
        curl -L ${UPDATE_SCRIPT_URL} -o ${PORTAL_DEST}update_tmp.sh > /dev/null 2>&1
        NEW_SCRIPT_VERS=`cat ${PORTAL_DEST}update_tmp.sh | grep ^VERSION= | sed 's/VERSION=//'`
        if [[ $NEW_SCRIPT_VERS -gt $OLD_SCRIPT_VERS ]]; then
            echo "----------- Updating update script. ------------"
            echo "------------------------------------------------"
            mv ${PORTAL_DEST}update_tmp.sh ${PORTAL_DEST}update.sh
            sudo chmod +x ${PORTAL_DEST}update.sh
        else
            rm ${PORTAL_DEST}update_tmp.sh
        fi;
    else
        echo "----------- Updating update script. ------------"
        echo "------------------------------------------------"
        curl -L ${UPDATE_SCRIPT_URL} -o ${PORTAL_DEST}update.sh > /dev/null 2>&1
        sudo chown $USER:root ${PORTAL_DEST}update.sh
        sudo chmod +x ${PORTAL_DEST}update.sh
    fi;
fi;
echo "---------- Checking uninstall script -----------"
echo "------------------------------------------------"
if ! [[ -e ${PORTAL_DEST}uninstall.sh ]]; then
    echo "--------- Fetching uninstall script. -----------"
    echo "------------------------------------------------"
    curl -L ${UNINSTALL_SCRIPT_URL} -o ${PORTAL_DEST}uninstall.sh > /dev/null 2>&1
    sudo chown $USER:root ${PORTAL_DEST}uninstall.sh
    sudo chmod +x ${PORTAL_DEST}uninstall.sh
else
    OLD_SCRIPT_VERS=`cat ${PORTAL_DEST}uninstall.sh | grep ^VERSION= | sed 's/VERSION=//'`
    if [[ $OLD_SCRIPT_VERS -ge 0 ]]; then
        curl -L ${UNINSTALL_SCRIPT_URL} -o ${PORTAL_DEST}uninstall_tmp.sh > /dev/null 2>&1
        NEW_SCRIPT_VERS=`cat ${PORTAL_DEST}uninstall_tmp.sh | grep ^VERSION= | sed 's/VERSION=//'`
        if [[ $NEW_SCRIPT_VERS -gt $OLD_SCRIPT_VERS ]]; then
            echo "---------- Updating uninstall script. ----------"
            echo "------------------------------------------------"
            mv ${PORTAL_DEST}uninstall_tmp.sh ${PORTAL_DEST}uninstall.sh
        else
            rm ${PORTAL_DEST}uninstall_tmp.sh
        fi;
    else
        echo "---------- Updating uninstall script. ----------"
        echo "------------------------------------------------"
        curl -L ${UNINSTALL_SCRIPT_URL} -o ${PORTAL_DEST}uninstall.sh > /dev/null 2>&1
        sudo chown $USER:root ${PORTAL_DEST}uninstall.sh
        sudo chmod +x ${PORTAL_DEST}uninstall.sh
    fi;
fi;
if ! [[ -e /etc/systemd/system/${SERVICE_NAME}.service ]]; then
    echo "----------- Creating portal service. -----------"
    echo "------------------------------------------------"
    sudo echo "    [Unit]" | sudo tee /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "Description=Chronokeep Portal Service" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "Wants=network-online.target" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "After=network.target network-online.target" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "StartLimitIntervalSec=0" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "[Service]" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "Type=simple" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "Restart=on-failure" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "RestartSec=1" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "User=$USER" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "ExecStart=${PORTAL_DEST}run.sh" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "[Install]" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
    sudo echo "WantedBy=multi-user.target" | sudo tee -a /etc/systemd/system/${SERVICE_NAME}.service
fi;
if ! [[ -e /etc/systemd/system/${QUIT_SERVICE_NAME}.service ]]; then
    echo "-------- Creating portal quit service. ---------"
    echo "------------------------------------------------"
    sudo echo "[Unit]" | sudo tee /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "Description=Ensure Chronokeep Portal closes before a server shutdown occurs." | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "DefaultDependencies=no" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "Before=shutdown.target" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo  | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "[Service]" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "Type=oneshot" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "ExecStart=${PORTAL_DEST}quit.sh" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "TimeoutStartSec=0" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo  | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "[Install]" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
    sudo echo "WantedBy=shutdown.target" | sudo tee -a /etc/systemd/system/${QUIT_SERVICE_NAME}.service
fi;
echo "--------- Setting base volume to 100%. ---------"
echo "------------------------------------------------"
amixer set 'PCM' 100% 2> /dev/null
if ! [[ -e ${PORTAL_DEST}/chronokeep-portal ]]; then
    echo "--------------- Fetching portal. ---------------"
    echo "------------------------------------------------"
    DOWNLOAD_URL=$(curl ${PORTAL_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://")
    curl -L ${DOWNLOAD_URL} -o ${PORTAL_DEST}release-portal.tar.gz 2> /dev/null
    if [[ $? -eq 0 ]]; then
        gunzip ${PORTAL_DEST}release-portal.tar.gz
        tar -xf ${PORTAL_DEST}release-portal.tar -C ${PORTAL_DEST}
        rm ${PORTAL_DEST}release-portal.tar
    fi;
fi;
if ! [[ -e ${PORTAL_DEST}/chronokeep-portal-quit ]]; then
    echo "------------- Fetching portal quit. ------------"
    echo "------------------------------------------------"
    DOWNLOAD_URL=$(curl ${QUIT_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://")
    curl -L ${DOWNLOAD_URL} -o ${PORTAL_DEST}release-portal-quit.tar.gz 2> /dev/null
    if [[ $? -eq 0 ]]; then
        gunzip ${PORTAL_DEST}release-portal-quit.tar.gz
        tar -xf ${PORTAL_DEST}release-portal-quit.tar -C ${PORTAL_DEST}
        rm ${PORTAL_DEST}release-portal-quit.tar
    fi;
fi;
echo "--------- Reloading systemctl daemons. ---------"
echo "------------------------------------------------"
sudo systemctl daemon-reload
echo "----------- Enabling portal service ------------"
echo "------------------------------------------------"
sudo systemctl enable ${SERVICE_NAME}
echo "----------- Starting portal service. -----------"
echo "------------------------------------------------"
sudo systemctl start ${SERVICE_NAME}
if ! [[ -e /etc/sudoers.d/chronoportal ]]; then
    echo "----------- Setting up nopasswd sudo -----------"
    echo "------------------------------------------------"
    if [[ -e /etc/sudoers.d/010_pi-nopasswd ]]; then
        sudo rm /etc/sudoers.d/010_pi-nopasswd
    fi;
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/bin/date" | sudo tee /etc/sudoers.d/chronoportal
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/reboot" | sudo tee -a /etc/sudoers.d/chronoportal
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/shutdown" | sudo tee -a /etc/sudoers.d/chronoportal
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/bin/systemctl" | sudo tee -a /etc/sudoers.d/chronoportal
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/hwclock" | sudo tee -a /etc/sudoers.d/chronoportal
fi;
echo "-------------- Setup is finished! --------------"
echo "------------------------------------------------"