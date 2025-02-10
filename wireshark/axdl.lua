axdl_protocol = Proto("AXDL",  "AXDL download protocol")

local marker = ProtoField.uint24("axdl.marker",   "Marker",   base.HEX)
local signature = ProtoField.uint32("axdl.signature",   "Signature",   base.HEX)
local frame_length = ProtoField.uint16("axdl.length",   "Length",   base.DEC)
local command = ProtoField.uint16("axdl.command",   "Command",   base.HEX)
local data = ProtoField.bytes("axdl.data",   "Data")
local start_address = ProtoField.uint32("axdl.start_address", "Start Address", base.HEX)
local total_length = ProtoField.uint32("axdl.total_length", "Total Length", base.DEC)
local start_address_64 = ProtoField.uint64("axdl.start_address", "Start Address", base.HEX)
local total_length_64 = ProtoField.uint64("axdl.total_length", "Total Length", base.DEC)
local partition_name = ProtoField.string("axdl.partition_name", "Partition Name")
local partition_table_header = ProtoField.uint64("axdl.partition_table_header", "Partition Table Header", base.HEX)
local partition_table_entry = ProtoField.bytes("axdl.partition_table_entry", "Partition Table Entry")
local partition_table_entry_name = ProtoField.string("axdl.partition_table_entry_name", "Partition Table Entry Name")
local partition_table_entry_gap = ProtoField.uint64("axdl.partition_table_entry_gap", "Partition Table Entry Gap")
local partition_table_entry_size = ProtoField.uint64("axdl.partition_table_entry_size", "Partition Table Entry Size")
local checksum = ProtoField.uint16("axdl.checksum",   "Check Sum",   base.HEX)


axdl_protocol.fields = { marker, signature, frame_length, command, data, start_address, total_length, start_address_64, total_length_64, partition_name, partition_table_header, partition_table_entry, partition_table_entry_name, partition_table_entry_gap, partition_table_entry_size, checksum }

-- Referenced USB URB dissector fields.
local f_urb_type = Field.new("usb.urb_type")
local f_transfer_type = Field.new("usb.transfer_type")
local f_endpoint = Field.new("usb.endpoint_address.number")
local f_direction = Field.new("usb.endpoint_address.direction")

function axdl_protocol.dissector(buffer, pinfo, tree)
  local transfer_type = tonumber(tostring(f_transfer_type()))
  if not(transfer_type == 3) then return 0 end
  
--   print("debug: " .. f_urb_type())
--   local urb_type = tonumber(tostring(f_urb_type()))
--   local endpoint = tonumber(tostring(f_endpoint()))
--   local direction = tonumber(tostring(f_direction()))

--   if     not(urb_type == 83 and endpoint == 1)   -- 'S' - Submit
--      and not(urb_type == 67 and endpoint == 1) -- 'C' - Complete
--         then
--     return 0
--   end
  
  length = buffer:len()
  if length < 3 then return end

  pinfo.cols.protocol = axdl_protocol.name

  local subtree = tree:add(axdl_protocol, buffer(), "AXDL")
  
  if length == 3 then
    subtree:add_le(marker, buffer(0, 3))
    return
  end

  if length < 10 then return end

  if buffer(0, 4):le_uint() ~= 0x5c6d8e9f then
    -- Data packet
    subtree:add(data, buffer(0, length))
    return
  end

  local n_command = buffer(6, 2):le_uint()
  local n_frame_length = buffer(4, 2):le_uint()
  local payload_length = length - 10

  subtree:add_le(signature,    buffer(0, 4))
  subtree:add_le(frame_length, buffer(4, 2))
  subtree:add_le(command,      buffer(6, 2))
  local data_subtree = subtree:add   (data, buffer(8, payload_length))
  if n_command == 0x0001 then
    if payload_length == 8  then
        data_subtree:add_le(start_address, buffer(8, 4))
        data_subtree:add_le(total_length, buffer(12, 4))
    end
    if payload_length == 16  then
        data_subtree:add_le(start_address_64, buffer(8, 8))
        data_subtree:add_le(total_length_64, buffer(16, 8))
    end
    if payload_length == 88 then
        data_subtree:add(partition_name, buffer(8, 72), buffer(8, 72):le_ustring())
        data_subtree:add_le(total_length, buffer(80, 4))
    end
  end
  if n_command == 0x000b then -- Partition Table
    data_subtree:add_le(partition_table_header, buffer(8, 8))
    for offset = 8, payload_length, 0x58 do
        if payload_length - offset < 0x58 then break end
        local entry = data_subtree:add(partition_table_entry, buffer(8 + offset, 0x58))
        entry:add(partition_table_entry_name, buffer(8 + offset, 0x40), buffer(8 + offset, 0x40):le_ustring())
        entry:add_le(partition_table_entry_gap, buffer(8 + offset + 0x40, 8))
        entry:add_le(partition_table_entry_size, buffer(8 + offset + 0x48, 8))
    end
  end
  subtree:add_le(checksum,     buffer(length - 2, 2))
end

function axdl_protocol.init()
    local usb_product_dissectors = DissectorTable.get("usb.product")

    -- Dissection by vendor+product ID requires that Wireshark can get the
    -- the device descriptor.  Making a USB device available inside a VM
    -- will make it inaccessible from Linux, so Wireshark cannot fetch the
    -- descriptor by itself.  However, it is sufficient if the guest requests
    -- the descriptor once while Wireshark is capturing.
    usb_product_dissectors:add(0x32c91000, axdl_protocol)

    -- Addendum: Protocol registration based on product ID does not always
    -- work as desired.  Register the protocol on the interface class instead.
    -- The downside is that it would be a bad idea to put this into the global
    -- configuration, so one has to make do with -X lua_script: for now.
    -- local usb_bulk_dissectors = DissectorTable.get("usb.bulk")

    -- For some reason the "unknown" class ID is sometimes 0xFF and sometimes
    -- 0xFFFF.  Register both to make it work all the time.
    -- usb_bulk_dissectors:add(0xFF, p_logic16)
    -- usb_bulk_dissectors:add(0xFFFF, p_logic16)
end

-- DissectorTable.get("usb.bulk"):add(0xffff, axdl_protocol)