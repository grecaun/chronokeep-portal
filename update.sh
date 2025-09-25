#!/bin/bash

DEST=/portal/
SERVICE_NAME=portal
FILES_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/files.sh'
UPDATE_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/update.sh'
UNINSTALL_SCRIPT_URL='https://raw.githubusercontent.com/grecaun/chronokeep-portal/main/uninstall.sh'
PORTAL_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal/releases/latest'
QUIT_REPO_URL='https://api.github.com/repos/grecaun/chronokeep-portal-quit/releases/latest'

VERSION=7

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

echo "------------------------------------------------"
echo "------------- Now updating Portal! -------------"
echo "------------------------------------------------"
echo "------------ Checking update script ------------"
echo "------------------------------------------------"
if ! [[ -e ${DEST}update.sh ]]; then
    echo "------------ Fetching update script ------------"
    echo "------------------------------------------------"
    curl -L ${UPDATE_SCRIPT_URL} -o ${DEST}update.sh > /dev/null 2>&1
    sudo chown $USER:root ${DEST}update.sh
    sudo chmod +x ${DEST}update.sh
    echo "------- Please re-run the updated script -------"
    echo "------------------------------------------------"
    exit 1
else
    OLD_SCRIPT_VERS=`cat ${DEST}update.sh | grep ^VERSION= | sed 's/VERSION=//'`
    if [[ $OLD_SCRIPT_VERS -ge 0 ]]; then
        curl -L ${UPDATE_SCRIPT_URL} -o ${DEST}update_tmp.sh > /dev/null 2>&1
        NEW_SCRIPT_VERS=`cat ${DEST}update_tmp.sh | grep ^VERSION= | sed 's/VERSION=//'`
        if [[ $NEW_SCRIPT_VERS -gt $OLD_SCRIPT_VERS ]]; then
            echo "------------ Updating update script ------------"
            echo "------------------------------------------------"
            mv ${DEST}update_tmp.sh ${DEST}update.sh
            sudo chown $USER:root ${DEST}update.sh
            sudo chmod +x ${DEST}update.sh
            echo "------- Please re-run the updated script -------"
            echo "------------------------------------------------"
            exit 1
        else
            rm ${DEST}update_tmp.sh
        fi;
    else
        echo "------------ Updating update script ------------"
        echo "------------------------------------------------"
        curl -L ${UPDATE_SCRIPT_URL} -o ${DEST}update.sh > /dev/null 2>&1
        sudo chown $USER:root ${DEST}update.sh
        sudo chmod +x ${DEST}update.sh
        echo "------- Please re-run the updated script -------"
        echo "------------------------------------------------"
        exit 1
    fi;
fi;
echo "--------- Checking file creation script --------"
echo "------------------------------------------------"
if ! [[ -e ${DEST}files.sh ]]; then
    echo "--------- Fetching file creation script --------"
    echo "------------------------------------------------"
    curl -L ${FILES_SCRIPT_URL} -o ${DEST}files.sh > /dev/null 2>&1
    sudo chown $USER:root ${DEST}files.sh
    sudo chmod +x ${DEST}files.sh
    sudo ${DEST}files.sh -f # Always run in force mode after update or create.
else
    OLD_SCRIPT_VERS=`cat ${DEST}files.sh | grep ^VERSION= | sed 's/VERSION=//'`
    if [[ $OLD_SCRIPT_VERS -ge 0 ]]; then
        curl -L ${FILES_SCRIPT_URL} -o ${DEST}files_tmp.sh > /dev/null 2>&1
        NEW_SCRIPT_VERS=`cat ${DEST}files_tmp.sh | grep ^VERSION= | sed 's/VERSION=//'`
        if [[ $NEW_SCRIPT_VERS -gt $OLD_SCRIPT_VERS ]]; then
            echo "--------- Updating file creation script --------"
            echo "------------------------------------------------"
            mv ${DEST}files_tmp.sh ${DEST}files.sh
            sudo chown $USER:root ${DEST}files.sh
            sudo chmod +x ${DEST}files.sh
            sudo ${DEST}files.sh -f # Always run in force mode after update or create.
        else
            rm ${DEST}files_tmp.sh
        fi;
    else
        echo "--------- Updating file creation script --------"
        echo "------------------------------------------------"
        curl -L ${FILES_SCRIPT_URL} -o ${DEST}files.sh > /dev/null 2>&1
        sudo chown $USER:root ${DEST}files.sh
        sudo chmod +x ${DEST}files.sh
        sudo ${DEST}files.sh -f # Always run in force mode after update or create.
    fi;
fi;
echo "---------- Checking uninstall script -----------"
echo "------------------------------------------------"
if ! [[ -e ${DEST}uninstall.sh ]]; then
    echo "---------- Fetching uninstall script -----------"
    echo "------------------------------------------------"
    curl -L ${UNINSTALL_SCRIPT_URL} -o ${DEST}uninstall.sh > /dev/null 2>&1
    sudo chown $USER:root ${DEST}uninstall.sh
    sudo chmod +x ${DEST}uninstall.sh
else
    OLD_SCRIPT_VERS=`cat ${DEST}uninstall.sh | grep ^VERSION= | sed 's/VERSION=//'`
    if [[ $OLD_SCRIPT_VERS -ge 0 ]]; then
        curl -L ${UNINSTALL_SCRIPT_URL} -o ${DEST}uninstall_tmp.sh > /dev/null 2>&1
        NEW_SCRIPT_VERS=`cat ${DEST}uninstall_tmp.sh | grep ^VERSION= | sed 's/VERSION=//'`
        if [[ $NEW_SCRIPT_VERS -gt $OLD_SCRIPT_VERS ]]; then
            echo "----------- Updating uninstall script ----------"
            echo "------------------------------------------------"
            mv ${DEST}uninstall_tmp.sh ${DEST}uninstall.sh
            sudo chown $USER:root ${DEST}uninstall.sh
            sudo chmod +x ${DEST}uninstall.sh
        else
            rm ${DEST}uninstall_tmp.sh
        fi;
    else
        echo "----------- Updating uninstall script ----------"
        echo "------------------------------------------------"
        curl -L ${UNINSTALL_SCRIPT_URL} -o ${DEST}uninstall.sh > /dev/null 2>&1
        sudo chown $USER:root ${DEST}uninstall.sh
        sudo chmod +x ${DEST}uninstall.sh
    fi;
fi;
# Check if the main Portal software is up to date.
echo "----- Checking latest Portal release version ---"
echo "------------------------------------------------"
LATEST_PORTAL=`curl ${PORTAL_REPO_URL} 2>&1 | grep tag_name | sed -e "s/[\":,]//g" | sed -e "s/tag_name//" | sed -e "s/v//"`
CURRENT_PORTAL='0.0.0'
if [[ -e ${DEST}version.txt ]]; then
    CURRENT_PORTAL=`cat ${DEST}version.txt | sed -e "s/v//"`
fi;
echo Latest portal version is ${LATEST_PORTAL}
echo "------------------------------------------------"
echo Current portal version is ${CURRENT_PORTAL}
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
    [[ ${LATEST_PORTAL_VERSION_MAJOR} -eq ${CURRENT_PORTAL_MAJOR} && ${LATEST_PORTAL_VERSION_MINOR} -eq ${CURRENT_PORTAL_MINOR} && ${LATEST_PORTAL_VERSION_PATCH} -gt ${CURRENT_PORTAL_PATCH} ]]; then
    echo "---- New version found! Updating Portal now ----"
    echo "------------------------------------------------"
    DOWNLOAD_URL=`curl ${PORTAL_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://"`
    curl -L ${DOWNLOAD_URL} -o ${DEST}release-portal.tar.gz
    gunzip ${DEST}release-portal.tar.gz
    tar -xf ${DEST}release-portal.tar -C ${DEST}
    rm ${DEST}release-portal.tar
    sudo systemctl restart ${SERVICE_NAME}
    echo "------------- Portal update complete -----------"
    echo "------------------------------------------------"
else
    echo "----------- Portal already up to date ----------"
    echo "------------------------------------------------"
fi

# Check if our Quit software is up to date as well.
echo "- Checking latest Portal Quit release version  -"
echo "------------------------------------------------"
LATEST_QUIT=`curl ${QUIT_REPO_URL} 2>&1 | grep tag_name | sed -e "s/[\":,]//g" | sed -e "s/tag_name//" | sed -e "s/v//"`
CURRENT_QUIT='0.0.0'
if [[ -e ${DEST}quit-version.txt ]]; then
    CURRENT_QUIT=`cat ${DEST}quit-version.txt | sed -e "s/v//"`
fi;
echo Latest Portal Quit version is ${LATEST_QUIT}
echo "------------------------------------------------"
echo Current Portal Quit version is ${CURRENT_QUIT}
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
    [[ ${LATEST_QUIT_VERSION_MAJOR} -eq ${CURRENT_QUIT_MAJOR} && ${LATEST_QUIT_VERSION_MINOR} -eq ${CURRENT_QUIT_MINOR} && ${LATEST_QUIT_VERSION_PATCH} -gt ${CURRENT_QUIT_PATCH} ]]; then
    echo "- New version found! Updating Portal Quit now. -"
    echo "------------------------------------------------"
    DOWNLOAD_URL=`curl ${QUIT_REPO_URL} 2>&1 | grep browser_download_url | grep ${TARGET} | sed -e "s/[\",]//g" | sed -e "s/browser_download_url://"`
    curl -L ${DOWNLOAD_URL} -o ${DEST}release-portal-quit.tar.gz
    gunzip ${DEST}release-portal-quit.tar.gz
    tar -xf ${DEST}release-portal-quit.tar -C ${DEST}
    rm ${DEST}release-portal-quit.tar
    echo "---------- Portal Quit update complete ---------"
    echo "------------------------------------------------"
else
    echo "--------- Portal Quit already up to date -------"
    echo "------------------------------------------------"
fi
echo "------------- Update is finished! --------------"
echo "------------------------------------------------"