wget -O initrd.img \
 https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/netboot-3.23.2/initramfs-virt

curl --unix-socket ./firecracker.socket -i \
 -X PUT 'http://localhost/boot-source' \
 -H 'Accept: application/json' \
 -H 'Content-Type: application/json' \
 -d "{
\"kernel_image_path\": \"$(pwd)/vmlinux.bin\",
    \"initrd_path\": \"$(pwd)/initrd.img\",
\"boot_args\": \"console=ttyS0 reboot=k panic=1 pci=off rdinit=/\"
}"

curl --unix-socket ./firecracker.socket -i \
 -X PUT 'http://localhost/actions' \
 -H 'Accept: application/json' \
 -H 'Content-Type: application/json' \
 -d '{ "action_type": "InstanceStart" }'
