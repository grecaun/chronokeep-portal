#!/bin/bash

PORTAL_DEST=/portal/
SERVICE_NAME=portal
UPDATE_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/update.sh'
UNINSTALL_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/uninstall.sh'
PORTAL_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal/releases/latest'
QUIT_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal-quit/releases/latest'

VERSION=1

# Check OS type & architecture
OS=$(uname)
if [[ $? -ne 0 ]] || [[ $OS -ne "Linux" ]]; then
    echo "This script is only designed to work on linux."
    exit 1
fi;
ARCH=$(uname -m)
if [[ $ARCH -ne "x86_64" ]] && [[ $ARCH -ne "armv7"* ]]; then
    echo "This script does not recognize the system's architecture."
    exit 1
fi;
if [[ $ARCH -eq "x86_64" ]]; then
    TARGET=amd64-linux
else
    TARGET=armv7-linux
fi;

echo "------------------------------------------------"
echo "---------- Now updating Chronoportal! ----------"
echo "------------------------------------------------"
echo "------------ Checking update script ------------"
echo "------------------------------------------------"
if ! [[ -e ${PORTAL_DEST}update.sh ]]; then
    echo "----------- Fetching update script. ------------"
    echo "------------------------------------------------"
    curl -L ${UPDATE_SCRIPT_URL} -o ${PORTAL_DEST}update.sh > /dev/null 2>&1
    sudo chown $USER:root ${PORTAL_DEST}update.sh
    sudo chmod +x ${PORTAL_DEST}update.sh
    echo "------- Please re-run the updated script -------"
    echo "------------------------------------------------"
    exit 1
else
    OLD_SCRIPT_VERS=`cat ${PORTAL_DEST}update.sh | grep VERSION= | sed 's/VERSION=//'`
    if [[ $OLD_SCRIPT_VERS -ge 1 ]]; then
        curl -L ${UPDATE_SCRIPT_URL} -o ${PORTAL_DEST}update_tmp.sh > /dev/null 2>&1
        NEW_SCRIPT_VERS=`cat ${PORTAL_DEST}update.sh | grep VERSION= | sed 's/VERSION=//'`
        if [[ $NEW_SCRIPT_VERS -gt $OLD_SCRIPT_VERS ]]; then
            echo "----------- Updating update script. ------------"
            echo "----- Going from $OLD_SCRIPT_VERS to $NEW_SCRIPT_VERS -----"
            echo "------------------------------------------------"
            mv ${PORTAL_DEST}update_tmp.sh ${PORTAL_DEST}update.sh
            echo "------- Please re-run the updated script -------"
            echo "------------------------------------------------"
            exit 1
        else
            rm ${PORTAL_DEST}update_tmp.sh
        fi;
    else
        echo "----------- Updating update script. ------------"
        echo "------------------------------------------------"
        curl -L ${UPDATE_SCRIPT_URL} -o ${PORTAL_DEST}update.sh > /dev/null 2>&1
        sudo chown $USER:root ${PORTAL_DEST}update.sh
        sudo chmod +x ${PORTAL_DEST}update.sh
        echo "------- Please re-run the updated script -------"
        echo "------------------------------------------------"
        exit 1
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
    OLD_SCRIPT_VERS=`cat ${PORTAL_DEST}uninstall.sh | grep VERSION= | sed 's/VERSION=//'`
    if [[ $OLD_SCRIPT_VERS -ge 1 ]]; then
        curl -L ${UNINSTALL_SCRIPT_URL} -o ${PORTAL_DEST}uninstall_tmp.sh > /dev/null 2>&1
        NEW_SCRIPT_VERS=`cat ${PORTAL_DEST}uninstall.sh | grep VERSION= | sed 's/VERSION=//'`
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

# Check if the main portal software is up to date.
echo "---- Checking latest portal release version. ---"
echo "------------------------------------------------"
LATEST_PORTAL=`curl ${PORTAL_REPO_URL} 2>&1 | grep tag_name | sed -e "s/[\":,]//g" | sed -e "s/tag_name//" | sed -e "s/v//"`
CURRENT_PORTAL='0.0.0'
if [[ -e ${PORTAL_DEST}version.txt ]]; then
    CURRENT_PORTAL=`cat ${PORTAL_DEST}version.txt | sed -e "s/v//"`
fi;
echo Latest portal version is ${LATEST_PORTAL} - current portal version is ${CURRENT_PORTAL}.
echo "------------------------------------------------"
LATEST_PORTAL_VERSION_MAJOR=`echo ${LATEST_PORTAL} | cut -d '.' -f 1`
LATEST_PORTAL_VERSION_MINOR=`echo ${LATEST_PORTAL} | cut -d '.' -f 2`
LATEST_PORTAL_VERSION_PATCH=`echo ${LATEST_PORTAL} | cut -d '.' -f 3`
CURRENT_PORTAL_MAJOR=`echo ${CURRENT_PORTAL} | cut -d '.' -f 1`
CURRENT_PORTAL_MINOR=`echo ${CURRENT_PORTAL} | cut -d '.' -f 2`
CURRENT_PORTAL_PATCH=`echo ${CURRENT_PORTAL} | cut -d '.' -f 3`
# If the latest version has a higher major version, update.
if [[ ${LATEST_PORTAL_VERSION_MAJOR} -gt ${CURRENT_PORTAL_MAJOR} ]] ||
        [[ ${LATEST_PORTAL_VERSION_MAJOR} -eq ${CURRENT_PORTAL_MAJOR} && ${LATEST_PORTAL_VERSION_MINOR} -gt ${CURRENT_PORTAL_MINOR} ]] ||
        [[ ${LATEST_PORTAL_VERSION_MAJOR} -eq ${CURRENT_PORTAL_major} && ${LATEST_PORTAL_VERSION_MINOR} -eq ${CURRENT_PORTAL_MINOR} && ${LATEST_PORTAL_VERSION_PATCH} -gt ${CURRENT_PORTAL_PATCH} ]]; then
        echo "---- New version found! Updating portal now. ---"
        echo "------------------------------------------------"
        DOWNLOAD_URL=`curl ${PORTAL_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://"`
        curl -L ${DOWNLOAD_URL} -o ${PORTAL_DEST}release-portal.tar.gz
        gunzip ${PORTAL_DEST}release-portal.tar.gz
        tar -xf ${PORTAL_DEST}release-portal.tar -C ${PORTAL_DEST}
        rm ${PORTAL_DEST}release-portal.tar
        sudo systemctl restart ${SERVICE_NAME}
        echo "------------ Portal update complete. -----------"
        echo "------------------------------------------------"
else
        echo "---------- Portal already up to date. ----------"
        echo "------------------------------------------------"
fi

# Check if our quit software is up to date as well.
echo "- Checking latest portal quit release version. -"
echo "------------------------------------------------"
LATEST_QUIT=`curl ${QUIT_REPO_URL} 2>&1 | grep tag_name | sed -e "s/[\":,]//g" | sed -e "s/tag_name//" | sed -e "s/v//"`
CURRENT_QUIT='0.0.0'
if [[ -e ${PORTAL_DEST}quit-version.txt ]]; then
    CURRENT_QUIT=`cat ${PORTAL_DEST}quit-version.txt | sed -e "s/v//"`
fi;
echo Latest portal quit version is ${LATEST_QUIT} - current portal quit version is ${CURRENT_QUIT}.
echo "------------------------------------------------"
LATEST_QUIT_VERSION_MAJOR=`echo ${LATEST_QUIT} | cut -d '.' -f 1`
LATEST_QUIT_VERSION_MINOR=`echo ${LATEST_QUIT} | cut -d '.' -f 2`
LATEST_QUIT_VERSION_PATCH=`echo ${LATEST_QUIT} | cut -d '.' -f 3`
CURRENT_QUIT_MAJOR=`echo ${CURRENT_QUIT} | cut -d '.' -f 1`
CURRENT_QUIT_MINOR=`echo ${CURRENT_QUIT} | cut -d '.' -f 2`
CURRENT_QUIT_PATCH=`echo ${CURRENT_QUIT} | cut -d '.' -f 3`
# If the latest version has a higher major version, update.
if [[ ${LATEST_QUIT_VERSION_MAJOR} -gt ${CURRENT_QUIT_MAJOR} ]] ||
        [[ ${LATEST_QUIT_VERSION_MAJOR} -eq ${CURRENT_QUIT_MAJOR} && ${LATEST_QUIT_VERSION_MINOR} -gt ${CURRENT_QUIT_MINOR} ]] ||
        [[ ${LATEST_QUIT_VERSION_MAJOR} -eq ${CURRENT_QUIT_major} && ${LATEST_QUIT_VERSION_MINOR} -eq ${CURRENT_QUIT_MINOR} && ${LATEST_QUIT_VERSION_PATCH} -gt ${CURRENT_QUIT_PATCH} ]]; then
        echo "- New version found! Updating portal quit now. -"
        echo "------------------------------------------------"
        DOWNLOAD_URL=`curl ${QUIT_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://"`
        curl -L ${DOWNLOAD_URL} -o ${PORTAL_DEST}release-portal-quit.tar.gz
        gunzip ${PORTAL_DEST}release-portal-quit.tar.gz
        tar -xf ${PORTAL_DEST}release-portal-quit.tar -C ${PORTAL_DEST}
        rm ${PORTAL_DEST}release-portal-quit.tar
        echo "--------- Portal quit update complete. ---------"
        echo "------------------------------------------------"
else
        echo "-------- Portal quit already up to date. -------"
        echo "------------------------------------------------"
fi
echo "------------- Update is finished! --------------"
echo "------------------------------------------------"