<?xml version="1.0" encoding="UTF-8"?>
<!-- Kenney's Pixel Platformer tiles (CC0) with the properties the game reads:
     `solid`, `one_way` and `hazard` for collision layers, and classes with
     `sensor` for the tiles placed as objects (coins, checkpoint flags, springs,
     the exit door). -->
<tileset version="1.10" tiledversion="1.10.2" name="tiles" tilewidth="18" tileheight="18" tilecount="180" columns="20">
 <image source="../images/tiles.png" width="360" height="162"/>
 <tile id="0"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="1"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="2"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="3"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="4"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="5"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="6"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="9"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="10"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="11"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="12"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="13"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="14"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="15"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="20"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="21"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="22"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="23"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="24"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="25"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="26"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="29"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="30"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="31"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="40"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="41"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="42"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="43"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="47"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="48"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="49"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="50"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="60"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="61"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="62"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="63"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="68"><properties><property name="hazard" type="bool" value="true"/></properties></tile>
 <tile id="80"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="81"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="82"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="83"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="100"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="101"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="102"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="103"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="104"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="107" type="Spring"><properties><property name="sensor" type="bool" value="true"/></properties></tile>
 <tile id="111" type="Checkpoint"><properties><property name="sensor" type="bool" value="true"/></properties><animation><frame tileid="111" duration="300"/><frame tileid="112" duration="300"/></animation></tile>
 <tile id="120"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="121"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="122"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="123"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="140"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="141"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="142"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="143"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="146"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="147"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="150" type="Exit"><properties><property name="sensor" type="bool" value="true"/></properties></tile>
 <tile id="151" type="Coin"><properties><property name="sensor" type="bool" value="true"/></properties><animation><frame tileid="151" duration="180"/><frame tileid="152" duration="120"/></animation></tile>
 <tile id="153"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="154"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="155"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
 <tile id="156"><properties><property name="one_way" type="bool" value="true"/></properties></tile>
</tileset>
