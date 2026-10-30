<?xml version="1.0" encoding="UTF-8"?>
<!-- Slopes made from Kenney's grass tile, sheared column by column: a 45-degree slope and
     the cell under it, the lower half of a 22.5-degree slope and the cell under it, and its
     upper half. Flip them horizontally in Tiled for slopes going down to the right. -->
<tileset version="1.10" tiledversion="1.10.2" name="slopes" tilewidth="18" tileheight="18" tilecount="5" columns="5">
 <image source="../images/slopes.png" width="90" height="18"/>
 <tile id="0"><properties><property name="slope_left" type="int" value="0"/><property name="slope_right" type="int" value="18"/></properties></tile>
 <tile id="1"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="2"><properties><property name="slope_left" type="int" value="0"/><property name="slope_right" type="int" value="9"/></properties></tile>
 <tile id="3"><properties><property name="solid" type="bool" value="true"/></properties></tile>
 <tile id="4"><properties><property name="slope_left" type="int" value="9"/><property name="slope_right" type="int" value="18"/></properties></tile>
</tileset>
