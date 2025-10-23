#! /bin/bash

DEST=/portal/
SERVICE_NAME=portal
QUIT_SERVICE_NAME=portal-quit
FILES_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/files.sh'
PORTAL_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal/releases/latest'
QUIT_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal-quit/releases/latest'

help_function()
{
    echo ""
    echo "Usage: $0 [-f]"
    echo -e "\t-f\tForces the creation of all scripts, service files, and sudoers file."
    exit 1
}

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

FORCE_CREATE=""
while getopts "f" opt
do
    case "$opt" in
        f) FORCE_CREATE="-f" ;;
        \?) help_function ;;
    esac
done

echo "------------------------------------------------"
echo "---------- Now installing Chronoportal ---------"
echo "------------------------------------------------"
echo "-------- Checking for required packages --------"
echo "------------------------------------------------"
curl -V > /dev/null 2>&1
if [[ $? != 0 ]]; then
    echo "---------------- Installing curl ---------------"
    echo "------------------------------------------------"
    sudo apt install curl -y
fi;
sudo apt list --installed 2>> /dev/null | grep alsa-utils > /dev/null 2>&1
if [[ $? != 0 ]]; then
    echo "------------- Installing alsa-utils ------------"
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
if ! [[ -e ${DEST} ]]; then
    echo "----------- Creating portal directory ----------"
    echo "------------------------------------------------"
    sudo mkdir ${DEST}
fi;
if ! [[ -e ${DEST}logs/ ]]; then
    echo "------------ Creating logs directory -----------"
    echo "------------------------------------------------"
    sudo mkdir ${DEST}logs/
fi;
sudo chown -R $USER:root ${DEST}
if ! [[ -e ${DEST}files.sh ]]; then
    curl -L ${FILES_SCRIPT_URL} -o ${DEST}files.sh > /dev/null 2>&1
fi;
${DEST}files.sh $FORCE_CREATE
echo "---------- Setting base volume to 100% ---------"
echo "------------------------------------------------"
amixer set 'PCM' 100% 2> /dev/null
if ! [[ -e ${DEST}/chronokeep-portal ]]; then
    echo "---------------- Fetching portal ---------------"
    echo "------------------------------------------------"
    DOWNLOAD_URL=$(curl ${PORTAL_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://")
    curl -L ${DOWNLOAD_URL} -o ${DEST}release-portal.tar.gz 2> /dev/null
    if [[ $? -eq 0 ]]; then
        gunzip ${DEST}release-portal.tar.gz
        tar -xf ${DEST}release-portal.tar -C ${DEST}
        rm ${DEST}release-portal.tar
    fi;
fi;
if ! [[ -e ${DEST}/chronokeep-portal-quit ]]; then
    echo "-------------- Fetching portal quit ------------"
    echo "------------------------------------------------"
    DOWNLOAD_URL=$(curl ${QUIT_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://")
    curl -L ${DOWNLOAD_URL} -o ${DEST}release-portal-quit.tar.gz 2> /dev/null
    if [[ $? -eq 0 ]]; then
        gunzip ${DEST}release-portal-quit.tar.gz
        tar -xf ${DEST}release-portal-quit.tar -C ${DEST}
        rm ${DEST}release-portal-quit.tar
    fi;
fi;
echo "---------- Reloading systemctl daemons ---------"
echo "------------------------------------------------"
sudo systemctl daemon-reload
echo "----------- Enabling portal service ------------"
echo "------------------------------------------------"
sudo systemctl enable ${SERVICE_NAME}
echo "------------ Starting portal service -----------"
echo "------------------------------------------------"
sudo systemctl start ${SERVICE_NAME}
if ! [[ -e /etc/sudoers.d/chronoportal ]] || [[ $FORCE_CREATE == true ]]; then
    echo "----------- Setting up nopasswd sudo -----------"
    echo "------------------------------------------------"
    if [[ -e /etc/sudoers.d/010_pi-nopasswd ]]; then
        sudo rm /etc/sudoers.d/010_pi-nopasswd
    fi;
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/bin/date" | sudo tee /etc/sudoers.d/chronoportal > /dev/null 2>&1
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/reboot" | sudo tee -a /etc/sudoers.d/chronoportal > /dev/null 2>&1
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/shutdown" | sudo tee -a /etc/sudoers.d/chronoportal > /dev/null 2>&1
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/bin/systemctl" | sudo tee -a /etc/sudoers.d/chronoportal > /dev/null 2>&1
    sudo echo "$USER ALL=(ALL) NOPASSWD: /usr/sbin/hwclock" | sudo tee -a /etc/sudoers.d/chronoportal > /dev/null 2>&1
fi;
echo "--------------- Install complete ---------------"
echo "------------------------------------------------"