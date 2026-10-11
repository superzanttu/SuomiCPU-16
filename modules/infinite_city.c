// Infinite City (Ikuinen kaupunki) SC-16-koneelle
//
// MIKA TAMA ON? (selitys 10-vuotiaalle):
//   Tama on peli, jossa juokset kaupungissa oikealle. Kaupunki ei lopu koskaan!
//   Vihollisia tulee vastaan, ja ammut niita raketeilla. Mita pidemmalle paaset
//   ja mita enemman vihollisia tuhoat, sita isompi pistemaara tulee.
//
// OHJAUS:
//   VASEN/OIKEA = juokse, VÄLILYÖNTI = hyppaa (pida pohjassa = korkeampi hyppy),
//   YLOS/ALAS = kiipea tikkaita / tippua katon lapi,
//   hiiri tahtaa, hiiren vasen nappi ampuu raketin.
//
// OHJELMAN RAKENNE:
//   1. Vakiot ja tietorakenteet (mita pelissa on: rakennukset, viholliset, raketit...)
//   2. Kaupungin luonti ja tormaykset (mihin pelaaja voi astua)
//   3. Paivitysfunktiot (mita tapahtuu joka kuvassa)
//   4. Piirtofunktiot (miten asiat nayttavat)
//   5. main-funktio: peli-silmukka, joka pyorii koko ajan
//
// ICON
// ################
// #..............#
// #..............#
// #..#...........#
// #..#..########.#
// #..#..#......#.#
// #..#..########.#
// #..#...........#
// #..#############
// #..............#
// #..##..##..##..#
// #..##..##..##..#
// #..............#
// #....########..#
// #..............#
// ################
#include "suomi_gfx.h"

// ---------- VAKIOT ----------
// Vakio on numero, jolle annetaan nimi. Silloin koodista nakee mita luku tarkoittaa.
#define BUILDING_MAX 20      // kuinka monta rakennusta on kerralla muistissa
#define ENEMY_MAX 10         // enintaan 10 vihollista yhtaaikaa
#define ROCKET_MAX 6         // raketteja yhteensa
#define PICKUP_MAX 8         // poimittavia laatikoita yhtaaikaa
#define BOMB_MAX 12          // lintujen pudottamia pommeja yhtaaikaa
#define BOOM_MAX 8           // rajahdyksia yhtaaikaa
#define PLAYER_W 12          // pelaajan leveys pikseleina
#define PLAYER_H 16          // pelaajan korkeus pikseleina
#define ENEMY_H 14           // vihollisen korkeus
#define GROUND_Y 200         // maan korkeus ruudulla (y kasvaa alaspain!)
#define JUMP_V 13            // hypyn alkunopeus (suurempi = korkeampi hyppy)
#define MOVE_SPEED 3         // pelaajan juoksunopeus
#define MAX_ROCKETS_AIR 4    // ilmassa saa olla enintaan 4 rakettia
#define WORLD_LIMIT 30000    // maailman raja (turvaraja)
#define START_LEVEL 4        // vaikeustaso pelin alussa
#define MAX_LEVEL 16         // vaikein taso

// Vihollistyypit
#define ENEMY_MRSIREN 0      // Herra Sireeni: kavelee ja jahtaa
#define ENEMY_MEGABIRD 1     // Megalintu: lentaa ja pudottaa pommeja
#define ENEMY_SPRINGU 2      // Springu: pomppiva jousi

// ---------- TIETORAKENTEET ----------
// "struct" on kuin lomake: se kokoaa yhden asian tiedot yhteen.

// Rakennus
typedef struct
{
    int x;            // vasen reuna maailmassa
    int w;            // leveys
    int roof_y;       // katon korkeus
    int ladder;       // 1 = rakennuksella on tikkaat
    int balcony;      // 1 = rakennuksella on parveke
    int balcony_y;    // parvekkeen korkeus
    int id;           // rakennuksen numero (vaikuttaa ulkonakoon)
} Building;

// Vihollinen
typedef struct
{
    int x;            // paikka sivusuunnassa
    int y;            // paikka ylos-alas
    int vx;           // nopeus sivusuunnassa
    int vy;           // nopeus ylos-alas
    int alive;        // 1 = elossa, 0 = ei ole kaytossa
    int kind;         // vihollistyyppi (ks. ENEMY_-vakiot)
    int hp;           // elamapisteet: kuinka monta osumaa kestaa
    int anim;         // animaatiolaskuri (liikuttaa jalkoja, siipia jne.)
    int phase;        // vihollisen tila (esim. jahtaa / odottaa)
    int on_ground;    // 1 = seisoo maassa
    int carried;      // 1 = lintu kantaa pelaajaa
    int bomb_cd;      // aika seuraavaan pommiin
    int dir;          // suunta: -1 vasen, 1 oikea
} Enemy;

// Raketti
typedef struct
{
    int x;
    int y;
    int vx;           // nopeus sivusuunnassa
    int vy;           // nopeus ylos-alas
    int life;         // kuinka kauan raketti vielä lentaa
    int active;       // 1 = lentaa
} Rocket;

// Linnun pudottama pommi
typedef struct
{
    int x;
    int y;
    int vy;           // putoamisnopeus
    int alive;
} Bomb;

// Rajahdys (pelkka kuva, ei vahingoita)
typedef struct
{
    int x;
    int y;
    int t;            // kuinka kauan rajahdys on kestanyt
    int big;          // 1 = iso rajahdys (pommi osui maahan)
    int alive;
} Boom;

// Pelaaja
typedef struct
{
    int x;
    int y;
    int vx;
    int vy;
    int on_ground;    // 1 = seisoo jollain pinnalla
    int on_ladder;    // 1 = kiipeaa tikkailla
    int facing;       // katsomissuunta
    int hp;           // elamat (100 alussa)
    int ammo;         // raketteja jaljella
    int ammo_timer;   // laskuri: uusi raketti ilmestyy ajan kuluttua
    int fire_cd;      // laukaisun jaahtyminen (ei voi ampua koko ajan)
    int walk;         // kavelyanimaation laskuri
    int fall_start;   // korkein kohta ilmassa (putoamisvahinkoa varten)
    int invuln;       // vahingoittumattomuusaika osuman jalkeen
    int carried_by;   // minka linnun kantama, -1 = ei kenenkaan
    int drop;         // aika jonka katot eivat pysayta (kun painat ALAS)
} Player;

// Poimittava raketti-laatikko
typedef struct
{
    int x;
    int y;
    int alive;
} Pickup;

// ---------- GLOBAALIT MUUTTUJAT ----------
// Taulukot, joissa on kaikki pelin oliot.
Building city[BUILDING_MAX];
Enemy enemies[ENEMY_MAX];
Rocket rockets[ROCKET_MAX];
Bomb bombs[BOMB_MAX];
Boom booms[BOOM_MAX];
Pickup pickups[PICKUP_MAX];
Player player;
int camera_x;        // kameran paikka: kuinka pitkalle kaupunkia on vieritetty
int frame_count;     // kuinka monta kuvaa on piirretty
int spawn_timer;     // aika seuraavaan vihollisen ilmestymiseen
int pickup_timer;    // aika seuraavaan laatikkoon
int pickup_flip;     // vuorottelee: katto / maa
int game_over;       // 1 = peli on loppu
int crosshair_x;     // tahtain (hiiren paikka)
int crosshair_y;
int kills_mr;        // tuhotut Herra Sireenit
int kills_bird;      // tuhotut Megalinnut
int kills_spring;    // tuhotut Springut
int rockets_fired;   // ammuttuja raketteja
int rockets_hit;     // osuneita raketteja
int world_end;       // kaupungin oikea reuna
int last_roof;       // edellisen rakennuksen katon korkeus
int build_count;     // kuinka monta rakennusta on tehty
int ladder_top;      // loydetyn tikkaan ylapaa
int max_x;           // pisin matka mihin pelaaja on paassyt
int meters;          // matka metreina
int best_meters;     // paras pistemaara (nimi on historiallinen)
int bonus;           // pisteet tuhotuista vihollisista
int new_best;        // 1 = uusi ennatys
int score;           // lopullinen pistemaara = matka + bonus
int level;           // vaikeustaso: kasvaa matkan mukana

// Muuttaa numeron tekstiksi, jotta sen voi piirtaa ruudulle.
// Esim. 123 -> "123". Tekee numerot yksi kerrallaan lopusta alkuun.
void number_text(int value, char *out)
{
    char tmp[16];
    int n = 0;
    int i = 0;
    if (value < 0)
    {
        out[0] = '-';
        i = 1;
        value = -value;
    }
    if (value == 0)
    {
        tmp[n++] = '0';
    }
    while (value > 0)
    {
        tmp[n++] = '0' + (value % 10);
        value = value / 10;
    }
    while (n > 0)
    {
        out[i++] = tmp[n - 1];
        n--;
    }
    out[i] = 0;
}

// Pitaa luvun alarajan (lo) ja ylarajan (hi) valissa.
int clamp_to_range(int value, int lo, int hi)
{
    if (value < lo)
    {
        return lo;
    }
    if (value > hi)
    {
        return hi;
    }
    return value;
}

// Luo rajahdyksen kohtaan x,y. Etsitaan ensin vapaa paikka taulukosta.
void add_boom(int x, int y)
{
    int i;
    for (i = 0; i < BOOM_MAX; i++)
    {
        if (!booms[i].alive)
        {
            booms[i].alive = 1;
            booms[i].x = x;
            booms[i].y = y;
            booms[i].t = 0;
            booms[i].big = 0;
            return;
        }
    }
}

// Iso rajahdys: kestaa pidempaan ja on paljon suurempi. Kaytetaan kun pommi osuu maahan.
void add_big_boom(int x, int y)
{
    int i;
    for (i = 0; i < BOOM_MAX; i++)
    {
        if (!booms[i].alive)
        {
            booms[i].alive = 1;
            booms[i].x = x;
            booms[i].y = y;
            booms[i].t = 0;
            booms[i].big = 1;
            gfx_sound(2, 60, 24, WAVE_NOISE, 60);
            return;
        }
    }
}

// Pelaaja saa vahinkoa. Osuman jalkeen pelaaja on hetken vahingoittumaton (inv),
// jotta kaikki elamat eivat lopu yhdella kertaa.
void hurt_player(int amount, int inv)
{
    if (player.invuln > 0 || game_over)
    {
        return;
    }
    // Korkeammalla tasolla vahinko on isompi
    player.hp -= amount + level / 3;
    player.invuln = inv;
    gfx_sound(3, 150, 8, WAVE_SQUARE, 40);
    if (player.hp <= 0)
    {
        player.hp = 0;
        game_over = 1;
    }
}

// Rakennusten teko: rakennukset luodaan vasemmalta oikealle. Kun rakennus jaa
// kauas kameran taakse, se tehdaan uudelleen kaupungin oikeaan paahan,
// joten kaupunki ei lopu koskaan (kuin liukuhihna!).
// Kattojen korkeus rajoitetaan niin, etta naapuritalon katolle paasee hyppaamalla
// (hyppy on noin 78 pikselia). Korkeimmille katoille pitaa kiivet tikkaita.
void gen_building(int i)
{
    int w = 28 + (gfx_random() % 40);       // leveys arvotaan
    int h = 50 + (gfx_random() % 80);       // korkeus arvotaan
    int roof = GROUND_Y - h;
    int x = world_end + 18 + (gfx_random() % 20);   // pieni rako edelliseen taloon
    int k;
    if (roof < last_roof - 64)
    {
        roof = last_roof - 64;   // ei liian paljon korkeampi kuin edellinen
    }
    if (roof < 60)
    {
        roof = 60;               // ei aivan ruudun ylareunaan
    }
    build_count++;
    k = build_count % 4;
    city[i].x = x;
    city[i].w = w;
    city[i].roof_y = roof;
    city[i].balcony_y = roof + 26;
    city[i].id = build_count;
    city[i].ladder = 0;
    city[i].balcony = 0;
    // Joka neljas talo: tikkaat. Joka neljas toinen: tikkaat ja parveke.
    if (k == 1)
    {
        city[i].ladder = 1;
    }
    if (k == 3)
    {
        city[i].ladder = 1;
        city[i].balcony = 1;
    }
    last_roof = roof;
    world_end = x + w;
}

// Tekee pelin alussa kaikki rakennukset.
void init_city(void)
{
    int i;
    world_end = 0;
    last_roof = 150;
    build_count = 0;
    for (i = 0; i < BUILDING_MAX; i++)
    {
        gen_building(i);
    }
}

// Uusii rakennukset, jotka ovat jaaneet kameran taakse.
void recycle_city(void)
{
    int i;
    for (i = 0; i < BUILDING_MAX; i++)
    {
        if (city[i].x + city[i].w < camera_x - 120 && world_end < WORLD_LIMIT)
        {
            gen_building(i);
        }
    }
}

// Kertoo katon korkeuden kohdassa wx (jos ei rakennusta, palauttaa maan korkeuden).
int building_roof_at_world(int wx)
{
    int i;
    for (i = 0; i < BUILDING_MAX; i++)
    {
        if (wx >= city[i].x && wx < city[i].x + city[i].w)
        {
            return city[i].roof_y;
        }
    }
    return GROUND_Y;
}

// Palauttaa tikkaiden x-paikan lahella kohtaa wx (tai -1 jos tikkaita ei ole)
// ja tallentaa tikkaiden ylapaan muuttujaan ladder_top.
int find_ladder(int wx)
{
    int i;
    int lx;
    for (i = 0; i < BUILDING_MAX; i++)
    {
        if (!city[i].ladder)
        {
            continue;
        }
        lx = city[i].x + city[i].w - 5;
        if (abs(wx - lx) < 8)
        {
            ladder_top = city[i].roof_y;
            return lx;
        }
    }
    return -1;
}

// Etsii pinnan (katto, parveke tai maa), jolle jalat osuvat pudotessa.
// Verrataan jalkojen vanhaa (feet_prev) ja uutta (feet) korkeutta:
// jos pinta jaa niiden valiin, jalat menivat pinnan lapi eli pelaaja laskeutuu.
// Palauttaa -1, jos mikaan pinta ei osunut kohdalle.
int find_support(int px, int feet_prev, int feet)
{
    int i;
    int s;
    int best = -1;
    if (feet_prev <= GROUND_Y && feet >= GROUND_Y)
    {
        best = GROUND_Y;
    }
    if (player.drop > 0)
    {
        // Pelaaja putoaa katon lapi: ohitetaan katot ja parvekkeet, vain maa pysayttaa
        return best;
    }
    for (i = 0; i < BUILDING_MAX; i++)
    {
        if (px >= city[i].x && px < city[i].x + city[i].w)
        {
            s = city[i].roof_y;
            if (feet_prev <= s && feet >= s && (best < 0 || s < best))
            {
                best = s;
            }
        }
        if (city[i].balcony && px >= city[i].x + city[i].w - 8 && px < city[i].x + city[i].w + 10)
        {
            s = city[i].balcony_y;
            if (feet_prev <= s && feet >= s && (best < 0 || s < best))
            {
                best = s;
            }
        }
    }
    return best;
}

// Lisaa poimittavan raketti-laatikon kohtaan x,y.
void spawn_pickup(int x, int y)
{
    int i;
    for (i = 0; i < PICKUP_MAX; i++)
    {
        if (!pickups[i].alive)
        {
            pickups[i].alive = 1;
            pickups[i].x = x;
            pickups[i].y = y;
            return;
        }
    }
}

// Aloittaa pelin alusta: nollaa pisteet, pelaajan ja kaikki viholliset.
void reset_game(void)
{
    int i;
    init_city();
    camera_x = 0;
    frame_count = 0;
    spawn_timer = 30;      // ensimmainen vihollinen tulee heti
    pickup_timer = 240;
    pickup_flip = 0;
    game_over = 0;
    max_x = 0;
    meters = 0;
    bonus = 0;
    score = 0;
    level = START_LEVEL;   // peli alkaa jo haastavalla tasolla
    new_best = 0;
    kills_mr = 0;
    kills_bird = 0;
    kills_spring = 0;
    rockets_fired = 0;
    rockets_hit = 0;
    player.x = 60;
    player.y = GROUND_Y - PLAYER_H;
    player.vx = 0;
    player.vy = 0;
    player.on_ground = 1;
    player.on_ladder = 0;
    player.facing = 1;
    player.hp = 100;
    player.ammo = 10;
    player.ammo_timer = 0;
    player.fire_cd = 0;
    player.walk = 0;
    player.fall_start = player.y;
    player.invuln = 0;
    player.carried_by = -1;
    player.drop = 0;
    crosshair_x = 160;
    crosshair_y = 120;
    for (i = 0; i < ENEMY_MAX; i++)
    {
        enemies[i].alive = 0;
    }
    for (i = 0; i < ROCKET_MAX; i++)
    {
        rockets[i].active = 0;
    }
    for (i = 0; i < BOMB_MAX; i++)
    {
        bombs[i].alive = 0;
    }
    for (i = 0; i < BOOM_MAX; i++)
    {
        booms[i].alive = 0;
    }
    for (i = 0; i < PICKUP_MAX; i++)
    {
        pickups[i].alive = 0;
    }
    spawn_pickup(220, GROUND_Y - 12);
    spawn_pickup(520, GROUND_Y - 12);
}

// Luo uuden vihollisen ruudun oikean reunan taakse. Tyyppi arvotaan:
// mita korkeampi taso, sita enemman vaarallisia lintuja.
void spawn_enemy(void)
{
    int i;
    int kind;
    int r;
    r = gfx_random() % 10;
    if (r < 4)
    {
        kind = ENEMY_MRSIREN;
    }
    else if (r < 7 - (level < 3 ? level : 3) || player.x < 200)
    {
        kind = ENEMY_SPRINGU;
    }
    else
    {
        kind = ENEMY_MEGABIRD;
    }
    for (i = 0; i < ENEMY_MAX; i++)
    {
        if (!enemies[i].alive)
        {
            enemies[i].alive = 1;
            enemies[i].kind = kind;
            enemies[i].x = camera_x + 330 + (gfx_random() % 40);
            enemies[i].y = GROUND_Y - ENEMY_H;
            enemies[i].vx = 0;
            enemies[i].vy = 0;
            enemies[i].hp = 1;
            enemies[i].anim = gfx_random() % 16;
            enemies[i].phase = 0;
            enemies[i].on_ground = 1;
            enemies[i].carried = 0;
            enemies[i].bomb_cd = 30;
            enemies[i].dir = -1;
            if (kind == ENEMY_MEGABIRD)
            {
                enemies[i].y = 50 + (gfx_random() % 60);
                enemies[i].hp = 2 + level / 5;
                enemies[i].on_ground = 0;
            }
            if (kind == ENEMY_SPRINGU)
            {
                enemies[i].phase = gfx_random() % 20;
            }
            return;
        }
    }
}

// Tuhoaa vihollisen numero j: antaa pisteita, tekee rajahdyksen
// ja joskus pudottaa raketti-laatikon.
void kill_enemy(int j)
{
    Enemy *e = &enemies[j];
    if (!e->alive)
    {
        return;
    }
    if (e->carried)
    {
        // Jos lintu kantoi pelaajaa, pelaaja vapautuu ja alkaa pudota
        e->carried = 0;
        player.carried_by = -1;
        player.vy = 0;
        player.on_ground = 0;
        player.fall_start = player.y;
    }
    e->alive = 0;
    // Pisteet: vaikeampi vihollinen ja korkeampi taso antavat enemman pisteita
    if (e->kind == ENEMY_MRSIREN)
    {
        kills_mr++;
        bonus += 5 + level;
    }
    else if (e->kind == ENEMY_MEGABIRD)
    {
        kills_bird++;
        bonus += 15 + level * 3;
    }
    else
    {
        kills_spring++;
        bonus += 10 + level * 2;
    }
    add_boom(e->x, e->y + 7);
    gfx_sound(2, 90, 12, WAVE_NOISE, 50);
    // Joka neljas vihollinen jattaa jalkeensa raketti-laatikon
    if (gfx_random() % 4 == 0)
    {
        if (e->kind == ENEMY_MEGABIRD)
        {
            spawn_pickup(e->x, building_roof_at_world(e->x) - 12);
        }
        else
        {
            spawn_pickup(e->x, GROUND_Y - 12);
        }
    }
}

// Ampuu raketin kohti tahtainta. Raketit elavat "maailman koordinaateissa"
// (kuten kaikki muukin paitsi tahtain ja pistetaulu), jotta ne pysyvat oikeassa
// paikassa kun kamera liikkuu.
void fire_rocket(void)
{
    int i;
    int dx;
    int dy;
    int ax;
    int ay;
    int length;
    int active_rockets = 0;
    // Ei voi ampua jos jaahtyminen kesken, ammukset loppu tai lintu kantaa
    if (player.fire_cd > 0 || player.ammo <= 0 || player.carried_by >= 0)
    {
        return;
    }
    // Lasketaan kuinka monta rakettia on jo ilmassa
    for (i = 0; i < ROCKET_MAX; i++)
    {
        if (rockets[i].active)
        {
            active_rockets++;
        }
    }
    if (active_rockets >= MAX_ROCKETS_AIR)
    {
        return;
    }
    // dx,dy = suunta pelaajasta tahtaimeen. length = pisin akseli,
    // jolla jaetaan niin etta raketin nopeus on aina suunnilleen 6.
    dx = crosshair_x - (player.x - camera_x + 6);
    dy = crosshair_y - (player.y + 8);
    ax = abs(dx);
    ay = abs(dy);
    length = ax;
    if (ay > length)
    {
        length = ay;
    }
    if (length < 6)
    {
        dx = player.facing * 6;
        dy = 0;
        length = 6;
    }
    for (i = 0; i < ROCKET_MAX; i++)
    {
        if (!rockets[i].active)
        {
            rockets[i].x = player.x + 6 + dx * 10 / length;
            rockets[i].y = player.y + 8 + dy * 10 / length;
            rockets[i].vx = dx * 6 / length;
            rockets[i].vy = dy * 6 / length;
            rockets[i].life = 110;
            rockets[i].active = 1;
            player.ammo--;
            player.fire_cd = 12;
            rockets_fired++;
            gfx_sound(1, 260, 6, WAVE_NOISE, 40);
            return;
        }
    }
}

// Kamera seuraa pelaajaa, mutta vain eteenpain (oikealle). Taakse ei voi palata.
void follow_camera(void)
{
    if (player.x - 120 > camera_x)
    {
        camera_x = player.x - 120;
    }
}

// Paivittaa pelaajan joka kuvassa: nappaimet, hyppy, painovoima, tikkaat, putoaminen.
void update_player(unsigned int keys)
{
    int lx;
    int feet_prev;
    int feet;
    int surf;
    int dir;
    int fall;
    if (game_over)
    {
        return;
    }
    if (player.fire_cd > 0)
    {
        player.fire_cd--;
    }
    if (player.invuln > 0)
    {
        player.invuln--;
    }
    if (player.drop > 0)
    {
        player.drop--;
    }
    if (player.carried_by >= 0)
    {
        follow_camera();
        return;
    }
    if (crosshair_x >= player.x - camera_x + 6)
    {
        player.facing = 1;
    }
    else
    {
        player.facing = -1;
    }
    feet_prev = player.y + PLAYER_H;
    lx = find_ladder(player.x + 6);
    if (player.on_ladder && lx < 0)
    {
        player.on_ladder = 0;
        player.fall_start = player.y;
    }
    if (!player.on_ladder && lx >= 0)
    {
        // Tikkaille noustaan painamalla YLOS tai ALAS tikkaiden kohdalla
        if (((keys & KEY_UP) && feet_prev > ladder_top && feet_prev <= GROUND_Y) ||
            ((keys & KEY_DOWN) && feet_prev >= ladder_top && feet_prev < GROUND_Y))
        {
            player.on_ladder = 1;
            player.vx = 0;
            player.vy = 0;
            player.on_ground = 0;
        }
    }
    if (player.on_ladder)
    {
        // Tikkailla: YLOS/ALAS liikuttaa pelaajaa 2 pikselia kerrallaan
        dir = 0;
        if (keys & KEY_UP)
        {
            dir = -1;
        }
        if (keys & KEY_DOWN)
        {
            dir = 1;
        }
        player.vx = 0;
        player.x = lx - 6;
        player.y += dir * 2;
        if (dir != 0)
        {
            player.walk++;
        }
        feet = player.y + PLAYER_H;
        if (feet <= ladder_top)
        {
            // Paastiin tikkaiden ylapaahan: astutaan katolle
            player.y = ladder_top - PLAYER_H;
            player.on_ladder = 0;
            player.on_ground = 1;
            player.vy = 0;
            player.fall_start = player.y;
        }
        else if (feet >= GROUND_Y)
        {
            // Paastiin alas maahan
            player.y = GROUND_Y - PLAYER_H;
            player.on_ladder = 0;
            player.on_ground = 1;
            player.vy = 0;
            player.fall_start = player.y;
        }
        else if (keys & KEY_FIRE)
        {
            // VALILYONTI tikkailla: hypataan pois tikkailta
            player.on_ladder = 0;
            player.vy = -6;
            player.fall_start = player.y;
            gfx_sound(0, 440, 5, WAVE_SQUARE, 25);
        }
        follow_camera();
        return;
    }
    // Vasen/oikea nappi asettaa sivuttaisnopeuden
    player.vx = 0;
    if (keys & KEY_LEFT)
    {
        player.vx -= MOVE_SPEED;
    }
    if (keys & KEY_RIGHT)
    {
        player.vx += MOVE_SPEED;
    }
    if ((keys & KEY_FIRE) && player.on_ground)
    {
        // Hyppy: annetaan ylospain nopeus (y pienenee ylos mentaessa)
        player.vy = -JUMP_V;
        player.on_ground = 0;
        player.fall_start = player.y;
        gfx_sound(0, 440, 5, WAVE_SQUARE, 25);
    }
    else if (!(keys & KEY_FIRE) && player.vy < -6)
    {
        // Jos valilyonti paastetaan irti kesken hypyn, hyppy jaa matalammaksi
        player.vy = -6;
    }
    if ((keys & KEY_DOWN) && player.on_ground && feet_prev < GROUND_Y)
    {
        // ALAS katolla: pudotaan katon lapi alas
        player.drop = 10;
        player.on_ground = 0;
        player.fall_start = player.y;
    }
    // Painovoima vetaa alas: nopeus kasvaa 1 joka kuva, mutta enintaan 9
    player.vy += 1;
    if (player.vy > 9)
    {
        player.vy = 9;
    }
    player.x += player.vx;
    player.y += player.vy;
    // Pelaaja ei voi menna ruudun vasemman reunan taakse
    if (player.x < camera_x + 4)
    {
        player.x = camera_x + 4;
    }
    if (player.x > WORLD_LIMIT)
    {
        player.x = WORLD_LIMIT;
    }
    feet = player.y + PLAYER_H;
    surf = -1;
    if (player.vy >= 0)
    {
        // Vain alaspain pudotessa voi laskeutua pinnalle
        surf = find_support(player.x + 6, feet_prev, feet);
    }
    if (surf >= 0)
    {
        // Laskeuduttiin: asetetaan pelaaja pinnan paalle
        player.y = surf - PLAYER_H;
        if (!player.on_ground)
        {
            // Putoamisvahinko: jos putosi yli 100 pikselia, sattuu
            fall = player.y - player.fall_start;
            if (fall > 100)
            {
                player.hp -= fall / 2 - 40;
                player.invuln = 20;
                gfx_sound(3, 120, 10, WAVE_NOISE, 50);
                if (player.hp <= 0)
                {
                    player.hp = 0;
                    game_over = 1;
                }
            }
        }
        player.on_ground = 1;
        player.vy = 0;
        player.fall_start = player.y;
    }
    else
    {
        if (player.on_ground)
        {
            player.fall_start = player.y;
        }
        player.on_ground = 0;
        if (player.y < player.fall_start)
        {
            player.fall_start = player.y;
        }
    }
    if (player.y < 0)
    {
        player.y = 0;
        if (player.vy < 0)
        {
            player.vy = 0;
        }
    }
    if (player.on_ground && player.vx != 0)
    {
        player.walk++;
    }
    follow_camera();
}

// Kaantaa pelaajan kuvan x-siirtyman katsomissuunnan mukaan,
// jotta pelaaja nayttaa katsovan vasemmalle tai oikealle.
int pm(int sx, int off, int w)
{
    if (player.facing >= 0)
    {
        return sx + off;
    }
    return sx + PLAYER_W - off - w;
}

// Piirtaa pelaajan kohtaan sx,sy (ruudun koordinaatit). Hahmo koostuu pienista
// suorakulmioista: jalat, vartalo, paa, laukku ja raketinheitin.
// Jalkojen asento riippuu siita kavelevatko, hyppaako vai kiipeaako pelaaja.
void draw_player(int sx, int sy)
{
    int s;
    int by;
    int la;
    int lb;
    int hl;
    int hr;
    int cx;
    int cy;
    int dxa;
    int dya;
    int ax;
    int ay;
    int len;
    int ex;
    int ey;
    if (player.invuln > 0 && (frame_count & 2))
    {
        // Vilkkuu osuman jalkeen: joka toinen kuva jaa piirtamatta
        return;
    }
    // s = kavelyanimaation vaihe 0..3
    s = (player.walk >> 2) & 3;
    by = sy;
    if (player.on_ground && player.vx != 0 && (s & 1))
    {
        by = sy + 1;
    }
    hl = 4;
    hr = 4;
    la = 3;
    lb = 6;
    if (player.on_ladder)
    {
        la = 2;
        lb = 6;
        if (s < 2)
        {
            hl = 3;
        }
        else
        {
            hr = 3;
        }
    }
    else if (!player.on_ground)
    {
        if (player.vy < 0)
        {
            la = 2;
            lb = 6;
            hl = 3;
        }
        else
        {
            la = 1;
            lb = 7;
        }
    }
    else if (player.vx != 0)
    {
        if (s == 0)
        {
            la = 1;
            lb = 7;
        }
        else if (s == 2)
        {
            la = 7;
            lb = 1;
        }
        else
        {
            la = 3;
            lb = 5;
        }
        if (s == 1)
        {
            hl = 3;
        }
        if (s == 3)
        {
            hr = 3;
        }
    }
    gfx_rect(pm(sx, la, 3), sy + 12, 3, hl, BLUE);
    gfx_rect(pm(sx, la, 4), sy + 11 + hl, 4, 1, RED);
    gfx_rect(pm(sx, lb, 3), sy + 12, 3, hr, BLUE);
    gfx_rect(pm(sx, lb, 4), sy + 11 + hr, 4, 1, RED);
    gfx_rect(pm(sx, -1, 3), by + 7, 3, 5, ORANGE);
    gfx_rect(pm(sx, 2, 8), by + 6, 8, 6, GREEN);
    gfx_rect(pm(sx, 2, 8), by + 11, 8, 1, YELLOW);
    gfx_rect(pm(sx, 3, 6), by + 3, 6, 3, YELLOW);
    gfx_rect(pm(sx, 2, 8), by, 8, 3, RED);
    gfx_rect(pm(sx, 6, 3), by + 3, 3, 2, CYAN);
    if (player.on_ladder)
    {
        if (s & 2)
        {
            gfx_rect(pm(sx, 0, 2), by + 3, 2, 5, YELLOW);
            gfx_rect(pm(sx, 10, 2), by + 7, 2, 5, YELLOW);
        }
        else
        {
            gfx_rect(pm(sx, 0, 2), by + 7, 2, 5, YELLOW);
            gfx_rect(pm(sx, 10, 2), by + 3, 2, 5, YELLOW);
        }
    }
    else
    {
        cx = sx + 6;
        cy = by + 8;
        dxa = crosshair_x - cx;
        dya = crosshair_y - cy;
        ax = abs(dxa);
        ay = abs(dya);
        if (ax > ay)
        {
            len = ax + ay / 2;
        }
        else
        {
            len = ay + ax / 2;
        }
        if (len < 1)
        {
            len = 1;
        }
        ex = cx + dxa * 11 / len;
        ey = cy + dya * 11 / len;
        gfx_line(cx, cy, ex, ey, GRAY);
        gfx_line(cx, cy + 1, ex, ey + 1, GRAY);
        gfx_rect(ex - 1, ey - 1, 3, 3, RED);
        gfx_rect(cx - 1, cy - 1, 3, 3, YELLOW);
    }
}

// Piirtaa koko kaupungin: taivas, tahdet, rakennukset (ikkunat, ovet, antennit,
// parvekkeet, tikkaat) ja maan. Rakennuksen paikka ruudulla = maailman x - kameran x.
void draw_city(void)
{
    int i;
    int k;
    int sx;
    int x0;
    int y;
    int row;
    int cell;
    int rows;
    int cells;
    int lx;
    int bx;
    int by;
    int roof;
    int w;
    gfx_rect(0, 0, 320, GROUND_Y, BLUE);   // sininen taivas
    // Tahdet liikkuvat hitaammin kuin kaupunki, joten tausta nayttaa kaukaiselta
    for (i = 0; i < 32; i++)
    {
        x0 = (i * 47 + 320 - (camera_x / 4) % 320) % 320;
        y = 44 + ((i * 13) & 63);
        gfx_pixel(x0, y, WHITE);
        gfx_pixel(x0 + 1, y - 2, WHITE);
    }
    for (i = 0; i < BUILDING_MAX; i++)
    {
        sx = city[i].x - camera_x;
        w = city[i].w;
        roof = city[i].roof_y;
        if (sx > 330 || sx + w < -20)
        {
            // Rakennus on ruudun ulkopuolella, ei piirreta
            continue;
        }
        gfx_rect(sx, roof, w, GROUND_Y - roof, DARK_GRAY);   // seina
        gfx_rect(sx, roof, w, 3, GRAY);                      // katto
        gfx_rect(sx + w - 3, roof + 3, 3, GROUND_Y - roof - 3, BLACK);  // varjo
        cells = (w - 16) / 10;
        rows = (GROUND_Y - roof - 22) / 14;
        // Ikkunat: joka rivilla ja sarakkeella, osa palaa (keltainen), osa ei (musta)
        for (row = 0; row < rows; row++)
        {
            for (cell = 0; cell < cells; cell++)
            {
                if (((row * 5) + (cell * 3) + city[i].id) % 4 != 0)
                {
                    gfx_rect(sx + 4 + cell * 10, roof + 8 + row * 14, 6, 8, YELLOW);
                }
                else
                {
                    gfx_rect(sx + 4 + cell * 10, roof + 8 + row * 14, 6, 8, BLACK);
                }
            }
        }
        gfx_rect(sx + w / 2 - 4, GROUND_Y - 12, 8, 12, BLACK);   // ovi
        if ((city[i].id % 3) == 0)
        {
            // Joillakin taloilla on antenni, jonka punainen valo vilkkuu
            gfx_line(sx + 6, roof - 10, sx + 6, roof, GRAY);
            if (frame_count & 16)
            {
                gfx_pixel(sx + 6, roof - 11, RED);
            }
        }
        if (city[i].balcony)
        {
            // Parveke: lattia, kaide ja tukipalkki
            bx = sx + w - 8;
            by = city[i].balcony_y;
            gfx_rect(bx, by, 18, 3, GRAY);
            gfx_line(bx + 17, by - 8, bx + 17, by, WHITE);
            gfx_line(bx + 8, by - 8, bx + 17, by - 8, WHITE);
            gfx_line(bx + 17, by + 3, bx + 8, by + 10, GRAY);
        }
        if (city[i].ladder)
        {
            // Tikkaat: kaksi pystytolppaa ja poikittaisia askelmia
            lx = sx + w - 5;
            gfx_rect(lx - 3, roof, 1, GROUND_Y - roof, GRAY);
            gfx_rect(lx + 3, roof, 1, GROUND_Y - roof, GRAY);
            for (y = roof + 4; y < GROUND_Y; y += 8)
            {
                gfx_line(lx - 3, y, lx + 3, y, CYAN);
            }
        }
    }
    gfx_rect(0, GROUND_Y, 320, 40, DARK_GRAY);   // katu
    gfx_line(0, GROUND_Y, 319, GROUND_Y, GRAY);
    // Tien keskiviiva liikkuu kameran mukana
    x0 = -(camera_x % 48);
    for (k = 0; k < 8; k++)
    {
        gfx_rect(x0 + k * 48, 220, 20, 2, GRAY);
    }
}

// Piirtaa pienen raketin kuvakkeen (kaytetaan pistetaulussa ja laatikoissa).
void draw_rocket_icon(int x, int y)
{
    gfx_pixel(x + 2, y, RED);
    gfx_rect(x + 1, y + 1, 3, 2, RED);
    gfx_rect(x + 1, y + 3, 3, 6, WHITE);
    gfx_pixel(x + 2, y + 4, CYAN);
    gfx_pixel(x + 2, y + 5, CYAN);
    gfx_rect(x, y + 7, 1, 3, RED);
    gfx_rect(x + 4, y + 7, 1, 3, RED);
    gfx_rect(x + 2, y + 9, 1, 2, ORANGE);
}

// Piirtaa raketti-laatikot. Laatikko "kelluu" ylos ja alas yhden pikselin.
void draw_pickups(void)
{
    int i;
    int sx;
    int sy;
    for (i = 0; i < PICKUP_MAX; i++)
    {
        if (!pickups[i].alive)
        {
            continue;
        }
        sx = pickups[i].x - camera_x;
        sy = pickups[i].y + ((frame_count >> 3) & 1);
        if (sx < -12 || sx > 330)
        {
            continue;
        }
        gfx_rect(sx, sy, 11, 13, DARK_RED);
        gfx_rect(sx + 1, sy + 1, 9, 11, BLACK);
        draw_rocket_icon(sx + 3, sy + 1);
    }
}

// Kaantaa vihollisen kuvan x-siirtyman sen kulkusuunnan mukaan.
int em(int sx, int dir, int off, int w)
{
    if (dir >= 0)
    {
        return sx + off;
    }
    return sx - off - w;
}

// Piirtaa vihollisen tyypin mukaan:
//   Herra Sireeni = kavelee, paassa vilkkuva sireenivalo
//   Megalintu     = lyo siipiaan, kantaessa kynnet roikkuvat alas
//   Springu       = jousi joka painuu kasaan ennen hyppya ja venyy ilmassa
void draw_enemy(Enemy *e)
{
    int sx;
    int sy;
    int dir;
    int s;
    int a;
    int b;
    int ch;
    int base;
    int hy;
    int k;
    int ya;
    int yb;
    int p;
    if (!e->alive)
    {
        return;
    }
    sx = e->x - camera_x;
    sy = e->y;
    dir = e->dir;
    if (sx < -24 || sx > 340)
    {
        return;
    }
    if (e->kind == ENEMY_MRSIREN)
    {
        s = (e->anim >> 2) & 3;
        a = 0;
        b = 0;
        if (e->phase)
        {
            if (s == 0)
            {
                a = -2;
                b = 2;
            }
            else if (s == 2)
            {
                a = 2;
                b = -2;
            }
        }
        if ((e->anim >> 3) & 1)
        {
            gfx_rect(sx - 2, sy, 4, 3, RED);
            gfx_pixel(sx - 5, sy + 1, RED);
            gfx_pixel(sx + 5, sy + 1, RED);
        }
        else
        {
            gfx_rect(sx - 2, sy, 4, 3, BLUE);
        }
        gfx_rect(sx - 1, sy + 3, 2, 1, GRAY);
        gfx_rect(sx - 5, sy + 4, 10, 5, MAGENTA);
        gfx_rect(em(sx, dir, 0, 4), sy + 5, 4, 2, WHITE);
        gfx_pixel(em(sx, dir, 2, 1), sy + 6, BLACK);
        gfx_rect(sx - 7, sy + 8, 14, 4, DARK_RED);
        gfx_rect(sx - 8, sy + 8 + (s & 1), 2, 3, GRAY);
        gfx_rect(sx + 6, sy + 9 - (s & 1), 2, 3, GRAY);
        gfx_rect(sx - 5 + a, sy + 12, 3, 2, YELLOW);
        gfx_rect(sx + 2 + b, sy + 12, 3, 2, YELLOW);
    }
    else if (e->kind == ENEMY_MEGABIRD)
    {
        s = (e->anim >> 1) & 3;
        if (s == 0)
        {
            a = -7;
        }
        else if (s == 2)
        {
            a = 4;
        }
        else
        {
            a = -1;
        }
        gfx_rect(sx - 6, sy + 1, 12, 6, GRAY);
        gfx_rect(sx - 4, sy + 6, 8, 2, WHITE);
        gfx_rect(em(sx, dir, -9, 4), sy + 2, 4, 2, DARK_GRAY);
        gfx_rect(em(sx, dir, 4, 5), sy - 1, 5, 5, GRAY);
        gfx_rect(em(sx, dir, 9, 3), sy + 1, 3, 2, YELLOW);
        gfx_pixel(em(sx, dir, 6, 1), sy, RED);
        gfx_line(sx - 2, sy + 2, sx - 13, sy + 2 + a, WHITE);
        gfx_line(sx - 2, sy + 3, sx - 13, sy + 3 + a, WHITE);
        gfx_line(sx + 2, sy + 2, sx + 13, sy + 2 + a, WHITE);
        gfx_line(sx + 2, sy + 3, sx + 13, sy + 3 + a, WHITE);
        if (e->carried)
        {
            gfx_rect(sx - 3, sy + 8, 2, 4, ORANGE);
            gfx_rect(sx + 1, sy + 8, 2, 4, ORANGE);
        }
        else
        {
            gfx_rect(sx - 2, sy + 8, 1, 2, ORANGE);
            gfx_rect(sx + 1, sy + 8, 1, 2, ORANGE);
        }
    }
    else
    {
        base = sy + ENEMY_H;
        ch = 9;
        if (e->on_ground)
        {
            if (e->phase < 0)
            {
                ch = 5;
            }
            else if (e->phase >= 24)
            {
                ch = 9 - (e->phase - 24) / 3;
            }
        }
        else if (e->vy < 0)
        {
            ch = 12;
        }
        else
        {
            ch = 10;
        }
        hy = base - ch - 8;
        gfx_rect(sx - 7, base - 2, 14, 2, BLUE);
        for (k = 0; k < 4; k++)
        {
            ya = base - 2 - (ch * k) / 4;
            yb = base - 2 - (ch * (k + 1)) / 4;
            if ((k & 1) == 0)
            {
                gfx_line(sx - 6, ya, sx + 6, yb, CYAN);
                gfx_line(sx - 6, ya - 1, sx + 6, yb - 1, CYAN);
            }
            else
            {
                gfx_line(sx + 6, ya, sx - 6, yb, CYAN);
                gfx_line(sx + 6, ya - 1, sx - 6, yb - 1, CYAN);
            }
        }
        gfx_rect(sx - 6, hy, 12, 6, GREEN);
        gfx_rect(sx - 4, hy + 1, 3, 3, WHITE);
        gfx_rect(sx + 1, hy + 1, 3, 3, WHITE);
        p = 0;
        if (dir > 0)
        {
            p = 1;
        }
        gfx_pixel(sx - 4 + p + 1, hy + 2, BLACK);
        gfx_pixel(sx + 1 + p + 1, hy + 2, BLACK);
        gfx_rect(sx - 3, hy + 5, 6, 1, RED);
    }
}

// Piirtaa linnun pudottamat pommit (musta pommi, sytytyslanka ja kipina).
void draw_bombs(void)
{
    int i;
    int sx;
    int sy;
    for (i = 0; i < BOMB_MAX; i++)
    {
        if (!bombs[i].alive)
        {
            continue;
        }
        sx = bombs[i].x - camera_x;
        sy = bombs[i].y;
        if (sx < -8 || sx > 340)
        {
            continue;
        }
        gfx_rect(sx - 3, sy - 2, 6, 5, BLACK);
        gfx_rect(sx - 2, sy - 3, 4, 7, BLACK);
        gfx_pixel(sx - 1, sy - 1, GRAY);
        gfx_line(sx, sy - 4, sx + 2, sy - 6, GRAY);
        if (frame_count & 2)
        {
            gfx_pixel(sx + 2, sy - 7, YELLOW);
        }
    }
}

// Piirtaa raketin lentosuuntaan kaannettyna: valkoinen runko, punainen karki ja
// siivet seka vilkkuva liekki perassa. vx,vy kertovat mihin suuntaan raketti menee.
void draw_rocket(Rocket *r)
{
    int sx;
    int sy;
    int nx;
    int ny;
    int tx;
    int ty;
    int fx;
    int fy;
    int ox;
    int oy;
    if (!r->active)
    {
        return;
    }
    sx = r->x - camera_x;
    sy = r->y;
    if (sx < -20 || sx > 340 || sy < -20 || sy > 260)
    {
        return;
    }
    nx = sx + r->vx;
    ny = sy + r->vy;
    tx = sx - r->vx;
    ty = sy - r->vy;
    ox = (-r->vy) / 2;
    oy = r->vx / 2;
    gfx_line(tx - r->vx * 2, ty - r->vy * 2, tx, ty, GRAY);
    gfx_line(tx, ty, nx, ny, WHITE);
    gfx_line(tx + ox / 3, ty + oy / 3, nx + ox / 3, ny + oy / 3, WHITE);
    gfx_line(tx - ox / 3, ty - oy / 3, nx - ox / 3, ny - oy / 3, WHITE);
    fx = tx + r->vx * 2 / 3;
    fy = ty + r->vy * 2 / 3;
    gfx_line(fx, fy, tx + ox, ty + oy, RED);
    gfx_line(fx, fy, tx - ox, ty - oy, RED);
    gfx_rect(nx - 1, ny - 1, 3, 3, RED);
    // Liekki vaihtaa vuorotellen oranssin ja keltaisen -> nayttaa liekehtivalta
    if (frame_count & 1)
    {
        gfx_rect(tx - r->vx / 2 - 1, ty - r->vy / 2 - 1, 3, 3, ORANGE);
    }
    else
    {
        gfx_rect(tx - r->vx / 2 - 1, ty - r->vy / 2 - 1, 3, 3, YELLOW);
    }
}

// Piirtaa rajahdykset. Rajahdys kasvaa ajan mukana ja vaihtaa varia:
// keltainen -> oranssi -> punainen.
void draw_booms(void)
{
    int i;
    int sx;
    int sy;
    int r;
    int d;
    int c;
    for (i = 0; i < BOOM_MAX; i++)
    {
        if (!booms[i].alive)
        {
            continue;
        }
        sx = booms[i].x - camera_x;
        sy = booms[i].y;
        r = 2 + booms[i].t * 2;
        if (booms[i].big)
        {
            r = 4 + booms[i].t * 4;   // iso rajahdys kasvaa kaksi kertaa nopeammin
        }
        c = RED;
        if (booms[i].t < 4)
        {
            c = YELLOW;
        }
        else if (booms[i].t < 7)
        {
            c = ORANGE;
        }
        d = r * 3 / 4 + 1;
        gfx_rect(sx - r / 2, sy - r / 2, r, r, c);
        gfx_line(sx - r - 2, sy, sx + r + 2, sy, c);
        gfx_line(sx, sy - r - 2, sx, sy + r + 2, c);
        gfx_line(sx - d, sy - d, sx + d, sy + d, c);
        gfx_line(sx - d, sy + d, sx + d, sy - d, c);
    }
}

// Piirtaa tahtaimen (hiiren kohdalle) ja "NO AMMO" -varoituksen kun raketit loppuvat.
void draw_crosshair(void)
{
    gfx_line(crosshair_x - 7, crosshair_y, crosshair_x - 3, crosshair_y, WHITE);
    gfx_line(crosshair_x + 3, crosshair_y, crosshair_x + 7, crosshair_y, WHITE);
    gfx_line(crosshair_x, crosshair_y - 7, crosshair_x, crosshair_y - 3, WHITE);
    gfx_line(crosshair_x, crosshair_y + 3, crosshair_x, crosshair_y + 7, WHITE);
    gfx_rect(crosshair_x - 1, crosshair_y - 1, 3, 3, RED);
    if (player.ammo <= 0 && (frame_count & 8))
    {
        gfx_text(crosshair_x + 10, crosshair_y + 6, "NO AMMO", RED);
    }
}

// Piirtaa ylhaalla olevan pistetaulun: elamat, pisteet, matka, taso, raketit.
void draw_hud(void)
{
    char s[32];
    int i;
    int c;
    gfx_rect(0, 0, 320, 38, DARK_GRAY);
    gfx_rect(0, 0, 320, 2, WHITE);
    number_text(player.hp, s);
    gfx_text(8, 5, "HP:", WHITE);
    gfx_text(38, 5, s, WHITE);
    number_text(score, s);
    gfx_text(80, 5, "SCORE", WHITE);
    gfx_text(120, 5, s, YELLOW);
    number_text(meters, s);
    gfx_text(170, 5, "DIST", WHITE);
    gfx_text(200, 5, s, WHITE);
    gfx_text(224, 5, "M", WHITE);
    number_text(level + 1, s);
    gfx_text(250, 5, "LVL", WHITE);
    gfx_text(274, 5, s, RED);
    c = GREEN;
    if (player.hp < 30)
    {
        c = RED;
    }
    gfx_rect(8, 15, 100, 5, BLACK);          // elamapalkin tausta
    gfx_rect(8, 15, player.hp, 5, c);        // elamapalkki: pituus = elamat
    gfx_text(8, 24, "AMMO", WHITE);
    for (i = 0; i < player.ammo; i++)
    {
        draw_rocket_icon(52 + i * 8, 24);
    }
    gfx_text(140, 26, "FIRED", WHITE);
    number_text(rockets_fired, s);
    gfx_text(190, 26, s, YELLOW);
    gfx_text(226, 26, "HITS", WHITE);
    number_text(rockets_hit, s);
    gfx_text(262, 26, s, YELLOW);
    // Pelin alussa nayteteaan ohje hetken (240 kuvaa = noin 4 sekuntia)
    if (frame_count < 240)
    {
        gfx_text(24, 120, "SPACE JUMP   MOUSE AIM + LEFT BUTTON FIRE", WHITE);
        gfx_text(24, 132, "UP/DOWN CLIMB LADDERS", WHITE);
    }
}

// Paivittaa kaikki viholliset: liike, tekoaly (ketä jahtaa) ja osumat pelaajaan.
// "adx" on vihollisen ja pelaajan etaisyys sivusuunnassa.
void update_enemies(void)
{
    int i;
    int b;
    int adx;
    int dxp;
    int dir;
    int target;
    int crouch;
    Enemy *e;
    // Springu valmistautuu hyppyyn lyhyemman aikaa mita korkeampi taso on
    crouch = 36 - level;
    if (crouch < 18)
    {
        crouch = 18;
    }
    for (i = 0; i < ENEMY_MAX; i++)
    {
        e = &enemies[i];
        if (!e->alive)
        {
            continue;
        }
        e->anim++;
        dxp = player.x + 6 - e->x;
        adx = abs(dxp);
        dir = 1;
        if (dxp < 0)
        {
            dir = -1;
        }
        if (e->kind == ENEMY_MRSIREN)
        {
            // Herra Sireeni: kavelee vasemmalle, mutta alkaa jahdata kun pelaaja on lahella
            if (adx < 110 + level * 4)
            {
                e->phase = 1;
            }
            else if (adx > 160 + level * 4)
            {
                e->phase = 0;
            }
            if (e->phase)
            {
                // Jahtaa pelaajaa, ja juoksee nopeammin kun on aivan lahella
                e->dir = dir;
                if (adx < 50)
                {
                    e->x += dir * (2 + level / 5);
                }
                else
                {
                    e->x += dir * (1 + level / 3);
                }
            }
            else
            {
                e->dir = -1;
                if ((e->anim & 1) == 0)
                {
                    e->x -= 1;
                }
            }
            if (player.invuln == 0 && adx < 11 && abs(player.y + 8 - (e->y + 7)) < 14)
            {
                // Sireeni osuu pelaajaan: pelaaja saa vahinkoa ja Sireeni rajahtaa
                hurt_player(12, 24);
                kill_enemy(i);
            }
        }
        else if (e->kind == ENEMY_MEGABIRD)
        {
            if (e->carried)
            {
                // Lintu kantaa pelaajaa ylos ja eteenpain, ja pudottaa hetken kuluttua
                e->phase++;
                if (e->y > 44)
                {
                    e->y--;
                }
                e->x += 1;
                player.x = e->x - 6;
                player.y = e->y + 10;
                player.vx = 0;
                player.vy = 0;
                player.on_ladder = 0;
                if (e->phase > 60)
                {
                    e->carried = 0;
                    player.carried_by = -1;
                    player.on_ground = 0;
                    player.fall_start = player.y;
                    e->phase = 100;
                }
            }
            else if (e->phase >= 100)
            {
                // Pudotuksen jalkeen lintu pakenee
                e->x -= 3;
                e->y -= 1;
            }
            else
            {
                // Lintu lentaa vasemmalle ja seuraa pelaajan korkeutta
                e->x -= 2 + level / 6;
                if (adx < 150 && (e->anim & 1))
                {
                    target = player.y - 10;
                    if (e->y < target)
                    {
                        e->y++;
                    }
                    else if (e->y > target)
                    {
                        e->y--;
                    }
                }
                e->y = clamp_to_range(e->y, 42, 170);
                if (e->bomb_cd > 0)
                {
                    e->bomb_cd--;
                }
                else if (adx < 28 && e->y < player.y - 4)
                {
                    // Pelaaja on suoraan alapuolella: pudotetaan pommi
                    for (b = 0; b < BOMB_MAX; b++)
                    {
                        if (!bombs[b].alive)
                        {
                            bombs[b].alive = 1;
                            bombs[b].x = e->x;
                            bombs[b].y = e->y + 9;
                            bombs[b].vy = 1;
                            break;
                        }
                    }
                    e->bomb_cd = 70 - level * 2;
                }
                if (player.carried_by < 0 && player.invuln == 0 && adx < 14 && abs(player.y + 4 - (e->y + 8)) < 14)
                {
                    // Lintu tarttuu pelaajaan kynsillaan
                    e->carried = 1;
                    e->phase = 0;
                    player.carried_by = i;
                    player.on_ladder = 0;
                    gfx_sound(3, 600, 6, WAVE_SQUARE, 40);
                }
            }
        }
        else
        {
            // Springu: odottaa (jousi painuu kasaan) ja hyppaa sitten pelaajaa kohti
            if (e->on_ground)
            {
                e->phase++;
                e->dir = dir;
                if (e->phase >= crouch && adx < 200)
                {
                    e->vy = -9;
                    if (player.y + PLAYER_H < e->y + ENEMY_H - 30)
                    {
                        // Pelaaja on korkealla: hypataan korkeammalle
                        e->vy = -12;
                    }
                    e->vx = dir * (2 + level / 8);
                    e->on_ground = 0;
                    e->phase = 0;
                }
                else if (e->phase > crouch)
                {
                    e->phase = crouch;
                }
            }
            else
            {
                // Ilmassa: painovoima vetaa alas, kunnes osutaan maahan
                e->x += e->vx;
                e->vy += 1;
                e->y += e->vy;
                if (e->y >= GROUND_Y - ENEMY_H)
                {
                    e->y = GROUND_Y - ENEMY_H;
                    e->vy = 0;
                    e->vx = 0;
                    e->on_ground = 1;
                    e->phase = -6;
                }
            }
            if (player.invuln == 0 && adx < 11 && abs(player.y + 8 - (e->y + 7)) < 13)
            {
                hurt_player(10, 24);
                player.vy = -5;
                player.on_ground = 0;
                player.on_ladder = 0;
                player.fall_start = player.y;
            }
        }
        if (e->alive && e->x < camera_x - 40)
        {
            if (e->carried)
            {
                kill_enemy(i);
            }
            else
            {
                e->alive = 0;
            }
        }
    }
}

// Paivittaa pommit: ne putoavat alas kiihtyvalla vauhdilla.
// Pommi putoaa kattojen ja parvekkeiden lapi. Se rajahtaa osuessaan pelaajaan
// (pieni rajahdys) tai maahan (iso rajahdys).
void update_bombs(void)
{
    int i;
    int prev;
    for (i = 0; i < BOMB_MAX; i++)
    {
        if (!bombs[i].alive)
        {
            continue;
        }
        prev = bombs[i].y;
        if (bombs[i].vy < 5 && (frame_count & 3) == 0)
        {
            bombs[i].vy++;
        }
        bombs[i].y += bombs[i].vy;
        if (abs(player.x + 6 - bombs[i].x) < 9 && abs(player.y + 8 - bombs[i].y) < 11)
        {
            hurt_player(15, 20);
            add_boom(bombs[i].x, bombs[i].y);
            bombs[i].alive = 0;
        }
        else if (prev < GROUND_Y && bombs[i].y >= GROUND_Y)
        {
            // Pommi lentaa kattojen lapi ja rajahtaa vasta maahan: iso rajahdys
            add_big_boom(bombs[i].x, GROUND_Y - 4);
            gfx_sound(2, 110, 8, WAVE_NOISE, 35);
            bombs[i].alive = 0;
        }
        else if (bombs[i].x < camera_x - 20)
        {
            bombs[i].alive = 0;
        }
    }
}

// Raketti-laatikko poimitaan koskemalla siihen: +5 rakettia (enintaan 10).
void update_pickups(void)
{
    int i;
    for (i = 0; i < PICKUP_MAX; i++)
    {
        if (!pickups[i].alive)
        {
            continue;
        }
        if (pickups[i].x < camera_x - 20)
        {
            pickups[i].alive = 0;
            continue;
        }
        if (abs(player.x + 6 - (pickups[i].x + 5)) < 12 && abs(player.y + 8 - (pickups[i].y + 6)) < 14)
        {
            player.ammo += 5;
            if (player.ammo > 10)
            {
                player.ammo = 10;
            }
            pickups[i].alive = 0;
            gfx_sound(0, 880, 6, WAVE_TRIANGLE, 40);
        }
    }
}

// Liikuttaa raketteja. Raketti lentaa kattojen ja parvekkeiden lapi. Se katoaa
// kun sen aika loppuu, se poistuu ruudulta, osuu maahan tai osuu viholliseen.
void update_rockets(void)
{
    int i;
    int j;
    int prev;
    int sx;
    Enemy *e;
    for (i = 0; i < ROCKET_MAX; i++)
    {
        if (!rockets[i].active)
        {
            continue;
        }
        prev = rockets[i].y;
        rockets[i].x += rockets[i].vx;
        rockets[i].y += rockets[i].vy;
        rockets[i].life--;
        sx = rockets[i].x - camera_x;
        if (rockets[i].life <= 0 || sx < -20 || sx > 340 || rockets[i].y < -20)
        {
            rockets[i].active = 0;
            continue;
        }
        if (prev < GROUND_Y && rockets[i].y >= GROUND_Y)
        {
            add_boom(rockets[i].x, GROUND_Y - 2);
            gfx_sound(2, 110, 8, WAVE_NOISE, 35);
            rockets[i].active = 0;
            continue;
        }
        for (j = 0; j < ENEMY_MAX; j++)
        {
            e = &enemies[j];
            if (!e->alive)
            {
                continue;
            }
            if (abs(rockets[i].x - e->x) < 11 && abs(rockets[i].y - (e->y + 7)) < 10)
            {
                rockets[i].active = 0;
                rockets_hit++;
                e->hp--;
                if (e->hp <= 0)
                {
                    kill_enemy(j);
                }
                else
                {
                    add_boom(rockets[i].x, rockets[i].y);
                    gfx_sound(2, 140, 6, WAVE_NOISE, 40);
                }
                break;
            }
        }
    }
}

// Paivittaa rajahdykset: ne kestavat 10 kuvaa ja katoavat.
void update_booms(void)
{
    int i;
    for (i = 0; i < BOOM_MAX; i++)
    {
        if (booms[i].alive)
        {
            booms[i].t++;
            if (booms[i].t > 9 && !booms[i].big)
            {
                booms[i].alive = 0;
            }
            else if (booms[i].t > 14)
            {
                booms[i].alive = 0;
            }
        }
    }
}

// Alapalkki: pelin nimi ja matka.
void draw_terrain_hud(void)
{
    char s[32];
    gfx_text(8, 228, "Infinite City", WHITE);
    number_text(meters, s);
    gfx_text(200, 228, "M:", WHITE);
    gfx_text(220, 228, s, YELLOW);
}

// Pelin loppuruutu: lopullinen pistemaara ja ennatys.
void end_game_screen(void)
{
    char s[32];
    gfx_rect(30, 70, 260, 100, DARK_GRAY);
    gfx_line(30, 70, 290, 70, WHITE);
    gfx_line(30, 170, 290, 170, WHITE);
    gfx_line(30, 70, 30, 170, WHITE);
    gfx_line(290, 70, 290, 170, WHITE);
    gfx_text(110, 78, "CITY LOST", WHITE);
    number_text(score, s);
    gfx_text(80, 100, "FINAL SCORE:", WHITE);
    gfx_text(190, 100, s, YELLOW);
    number_text(best_meters, s);
    gfx_text(80, 120, "BEST:", WHITE);
    gfx_text(190, 120, s, WHITE);
    if (new_best)
    {
        gfx_text(100, 138, "NEW BEST SCORE!", YELLOW);
    }
    gfx_text(100, 154, "PRESS START", WHITE);
}

// PAAOHJELMA: peli-silmukka. Se toistuu koko ajan, noin 60 kertaa sekunnissa.
// Joka kierroksella: 1) luetaan nappaimet ja hiiri, 2) paivitetaan peli,
// 3) piirretaan kaikki ruudulle.
int main(void)
{
    unsigned int keys;
    int i;
    int interval;
    reset_game();
    while (1)
    {
        keys = gfx_keys();
        crosshair_x = clamp_to_range(gfx_mouse_x(), 0, 319);
        crosshair_y = clamp_to_range(gfx_mouse_y(), 0, 239);
        if (!game_over)
        {
            update_player(keys);
            if (player.x > max_x)
            {
                max_x = player.x;
            }
            meters = max_x / 10;           // 10 pikselia = 1 metri
            // Vaikeustaso nousee 1 joka 40. metrilla, alkaen START_LEVEL:sta
            level = START_LEVEL + meters / 40;
            if (level > MAX_LEVEL)
            {
                level = MAX_LEVEL;
            }
            // Lopullinen pistemaara = matka metreina + vihollisista saadut pisteet
            score = meters + bonus;
            if (score > best_meters)
            {
                best_meters = score;
                new_best = 1;
            }
            // Hiiren vasen nappi ampuu
            if (gfx_mouse_buttons() & MOUSE_LEFT)
            {
                fire_rocket();
            }
            // Vihollisia ilmestyy sita useammin mita korkeampi taso on
            spawn_timer--;
            if (spawn_timer <= 0)
            {
                spawn_enemy();
                if (level >= 4 && gfx_random() % 3 == 0)
                {
                    spawn_enemy();   // joskus kaksi kerralla
                }
                interval = 80 - level * 4;
                if (interval < 20)
                {
                    interval = 20;
                }
                spawn_timer = interval;
            }
            // Raketti-laatikko ilmestyy vuorotellen katolle ja maahan
            pickup_timer--;
            if (pickup_timer <= 0)
            {
                pickup_flip = 1 - pickup_flip;
                if (pickup_flip)
                {
                    spawn_pickup(camera_x + 345, building_roof_at_world(camera_x + 345) - 12);
                }
                else
                {
                    spawn_pickup(camera_x + 345, GROUND_Y - 12);
                }
                pickup_timer = 360;
            }
            // Raketteja palautuu hitaasti itsestaan, ettei peli jaa jumiin (enintaan 3)
            player.ammo_timer++;
            if (player.ammo >= 3 || player.ammo_timer > 240)
            {
                if (player.ammo < 3)
                {
                    player.ammo++;
                }
                player.ammo_timer = 0;
            }
            // Paivitetaan kaikki pelin osat yksi kerrallaan
            update_enemies();
            update_bombs();
            update_pickups();
            update_rockets();
            update_booms();
            recycle_city();
        }
        else
        {
            // Peli on loppu: START-nappi aloittaa uuden pelin
            update_booms();
            if (keys & KEY_START)
            {
                reset_game();
            }
        }
        draw_city();    // piirretaan taustasta eteenpain: kauimmainen ensin
        draw_pickups();
        for (i = 0; i < ENEMY_MAX; i++)
        {
            draw_enemy(&enemies[i]);
        }
        draw_bombs();
        for (i = 0; i < ROCKET_MAX; i++)
        {
            draw_rocket(&rockets[i]);
        }
        draw_player(player.x - camera_x, player.y);
        draw_booms();
        draw_crosshair();
        draw_hud();
        draw_terrain_hud();
        if (game_over)
        {
            end_game_screen();
        }
        gfx_present();    // nayttaa valmiin kuvan ruudulla
        frame_count++;
    }
    return 0;
}
