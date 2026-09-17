# Chunker comparison: the 008 contract chunker vs chonky, same 2,000 articles

**A spike, 2026-09-17**, after Feature 021 — to answer "should the shipped Wikipedia index
(built by the Rust CLI with the contract chunker) move to chonky?" with numbers rather than a
guess. Two artefacts on disk, built from the same first 2,000 snapshot articles:
`target/xt-wiki-slice-rs` (`xtriever wiki build --limit 2000`, the 008 contract chunker —
8,529 passages, every one within the embedder's 256-position window) and
`target/xt-wiki-slice-chonky` (`wikidemo build --limit 2000`, chonky — 8,487 passages, 784
over the window). The twenty measurement queries (`reference/fixtures/008/queries.json`),
k = 10, depths 0 and 10, the same models. Heads are compared by **article** (passage ids
differ by construction); chonky's head passages are checked against the window.

Reproduce: `apps/python-wiki-demo/.venv/bin/python reference/chunker_compare.py --contract
target/xt-wiki-slice-rs --chonky target/xt-wiki-slice-chonky --out <file>`.

## Reading

- **The heads differ substantially**: mean Jaccard of the head articles 0.52; the top article
  agrees on 15 of 20 queries. Chunking is not a detail — it moves half of what a person sees.
- **Over-window passages reach the head at their corpus rate**: 21 of 200 head passages (10.5 %)
  exceed the window (the corpus share is 9.2 %). They get there through BM25 (a long chunk
  matches many terms) while their embedding covers only the first 256 word-pieces — e.g.
  q08 *Berlin* `2922#3` (1,872 positions), q15 *Tree* `847#4` (3,191), q18 *Chess* `3259#3`
  (5,145): passages whose head has little to do with the query. The trade-off accepted for
  the demo is visible in results, not only in a count.
- **Chonky's head passages are shorter where it matters**: median 88 positions vs 213 for the
  contract chunker — when chonky finds the break, the passage a person reads is tighter
  (q19 *Water vapor* first instead of an SI-units table; q16 *Black hole* `3506#1`).
- **Chonky also yields fragments**: q16 *Black hole* `3506#3` is 7 positions — `/ref>`, a
  markup remnant the splitter cut off as its own paragraph; the contract chunker never emits
  a passage that small because it fills to the budget.
- **Where the top article differs (q08, q10, q15, q17, q19)** neither side is consistently
  better by eye: q19 favours chonky, q17 favours the contract chunker (*England* `3047#10`
  names Shakespeare as a playwright; chonky's head is Austen/Socrates), q08/q10/q15 are a
  wash (the slice lacks the articles that would answer them).

**Conclusion for the shipped index**: no move. The semantic breaks help the head when they
land, but the unbounded chunks and the fragments cost as much as they give on this sample,
and Wikipedia has no relevance judgments to settle it beyond eyeballing. Two changes to the
demo's build would be needed before re-measuring — a bounded fallback for over-window
chunks (split at blank lines, then sentences, up to the window) and a minimum passage
length that merges fragments into their neighbour — and only after that a rebuilt slice
compared the same way. The Rust build, `xtriever-analysis::chunk` and the phone stay as
they are.

## Summary

| depth | mean Jaccard of head articles | top-1 article same | chonky head passages over the window | head passage positions, contract (median / max) | head passage positions, chonky (median / max) |
|---|---|---|---|---|---|
| 0 | 0.52 | 15/20 | 21/200 | 213 / 256 | 88 / 5145 |
| 10 | 0.52 | 15/20 | 21/200 | 213 / 256 | 88 / 5145 |

## Per query (top 5; `*` = over the 256-position window)

### q01 — why is the sky blue

**depth 0** — articles in both heads (Jaccard) 0.62; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Sky `2004#0` (229) — The sky is the appearance of the atmosphere around the surface of the planet from our poin… | Sky `2004#3` (14) — Why is the sky blue? References Basic English 850 words |
| 2 | Sky `2004#1` (106) — Many things can be seen in the sky. There are objects from space like the Sun, Moon, plane… | Sky `2004#0` (149) — The sky is the appearance of the atmosphere around the surface of the planet from our poin… |
| 3 | Blue `3697#0` (226) — Blue is one of the colors of the rainbow that people can see. It is one of the primary col… | Blue `3697#0` (198) — Blue is one of the colors of the rainbow that people can see. It is one of the primary col… |
| 4 | Adjective `3318#0` (233) — {{ExamplesSidebar|35%| I like blue skies and fluffy clouds. He is a nice man. It was a ver… | Sky `2004#1` (83) — Depending on the time of day, the sky may appear different colors. At dawn or dusk the sky… |
| 5 | Indigo `5556#0` (256) — Indigo is a shade of blue, more specifically, purplish blue or dark blue. Isaac Newton nam… | Sky `2004#2` (95) — Many things can be seen in the sky. There are objects from space like the Sun, Moon, plane… |

**depth 10** — articles in both heads (Jaccard) 0.62; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Sky `2004#0` (229) — The sky is the appearance of the atmosphere around the surface of the planet from our poin… | Sky `2004#0` (149) — The sky is the appearance of the atmosphere around the surface of the planet from our poin… |
| 2 | Sky `2004#1` (106) — Many things can be seen in the sky. There are objects from space like the Sun, Moon, plane… | Sky `2004#3` (14) — Why is the sky blue? References Basic English 850 words |
| 3 | Blue `3697#0` (226) — Blue is one of the colors of the rainbow that people can see. It is one of the primary col… | Blue `3697#0` (198) — Blue is one of the colors of the rainbow that people can see. It is one of the primary col… |
| 4 | Adjective `3318#0` (233) — {{ExamplesSidebar|35%| I like blue skies and fluffy clouds. He is a nice man. It was a ver… | Adjective `3318#2` (118) — Sometimes an adjective is not followed by a noun: The sky is blue. The joke she told was s… |
| 5 | Indigo `5556#0` (256) — Indigo is a shade of blue, more specifically, purplish blue or dark blue. Isaac Newton nam… | Sky `2004#1` (83) — Depending on the time of day, the sky may appear different colors. At dawn or dusk the sky… |


### q02 — who was the first person to walk on the moon

**depth 0** — articles in both heads (Jaccard) 0.42; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Astronaut `4783#0` (203) — An astronaut or cosmonaut is a person who goes into outer space. The Soviet Union and coun… | Astronaut `4783#1` (81) — The first person to go into space was a Russian from the Soviet Union. His name was Yuri G… |
| 2 | Soviet Union `3600#12` (129) — During this period of the late 1950s and early 1960s, the Soviet Union continued to make p… | Buzz Aldrin `4759#0` (56) — Dr. Edwin Eugene "Buzz" Aldrin, Jr., retired Colonel (born January 20, 1930) is an America… |
| 3 | Buzz Aldrin `4759#0` (230) — Dr. Edwin Eugene "Buzz" Aldrin, Jr., retired Colonel (born January 20, 1930) is an America… | Phoebe (moon) `5093#0` (85) — Phoebe is a moon which goes around (orbits) the planet called Saturn. It takes eighteen mo… |
| 4 | Natural satellite `2900#1` (213) — Moons do not make their own light. We can see the Earth's moon because it acts like a mirr… | Natural satellite `2900#2` (114) — A moon's cycle is the time the moon takes to change from looking very bright and round to … |
| 5 | Galileo Galilei `4052#1` (228) — Astronomy Some people believe that Galileo was the first person to build a telescope. This… | Phoebe (moon) `5093#1` (94) — There are many craters on Phoebe. These are from asteroids and other things crashing into … |

**depth 10** — articles in both heads (Jaccard) 0.42; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Astronaut `4783#0` (203) — An astronaut or cosmonaut is a person who goes into outer space. The Soviet Union and coun… | Astronaut `4783#1` (81) — The first person to go into space was a Russian from the Soviet Union. His name was Yuri G… |
| 2 | Soviet Union `3600#12` (129) — During this period of the late 1950s and early 1960s, the Soviet Union continued to make p… | Buzz Aldrin `4759#0` (56) — Dr. Edwin Eugene "Buzz" Aldrin, Jr., retired Colonel (born January 20, 1930) is an America… |
| 3 | Buzz Aldrin `4759#0` (230) — Dr. Edwin Eugene "Buzz" Aldrin, Jr., retired Colonel (born January 20, 1930) is an America… | Natural satellite `2900#2` (114) — A moon's cycle is the time the moon takes to change from looking very bright and round to … |
| 4 | July `402#8` (249) — July 19 1903: Maurice Garin wins the first Tour de France. July 20 1810: Bogota, New Grana… | Buzz Aldrin `4759#2` (110) — Apollo 11 mission Aldrin was the second person in history to set foot on the Moon (after N… |
| 5 | Galileo Galilei `4052#1` (228) — Astronomy Some people believe that Galileo was the first person to build a telescope. This… | Galileo Galilei `4052#2` (162) — Astronomy Some people believe that Galileo was the first person to build a telescope. This… |


### q03 — how do vaccines work

**depth 0** — articles in both heads (Jaccard) 0.50; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Immunology `394#1` (229) — Innate immune response The innate immune system is usually means all of the cells and syst… | Immunology `394#7` (57) — Other aspects of immunity Vaccines boost the acquired immune system by offering weak forms… |
| 2 | Lymphocyte `6034#1` (247) — T and B cells T cells (thymus cells) and B cells (bone cells) are the main cells of the ad… | Lymphocyte `6034#3` (74) — Once they are made active, B cells and T cells produce memory cells. Throughout the lifeti… |
| 3 | Human penis `4465#8` (146) — Vaccination to reduce the spread of sexually transmitted infections Vaccinating young peop… | Immunology `394#8` (66) — The distribution of vaccines and other immune system affecting cures can be considered ano… |
| 4 | Ebola virus `242#4` (128) — Prevention In December 2016, a study found the VSV-EBOV vaccine to be very effective (in t… | Tuberculosis `4760#2` (85) — There is a vaccine against some forms of tuberculosis. It is called the Bacillus Calmette–… |
| 5 | Tuberculosis `4760#1` (191) — There is a vaccine against some forms of tuberculosis. It is called the Bacillus Calmette–… | Ebola virus `242#5` (551*) — 3. The virus then, goes on to attack spleen, kidneys and even the brain. The blood vessels… |

**depth 10** — articles in both heads (Jaccard) 0.50; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Immunology `394#1` (229) — Innate immune response The innate immune system is usually means all of the cells and syst… | Immunology `394#7` (57) — Other aspects of immunity Vaccines boost the acquired immune system by offering weak forms… |
| 2 | Human penis `4465#8` (146) — Vaccination to reduce the spread of sexually transmitted infections Vaccinating young peop… | Lymphocyte `6034#3` (74) — Once they are made active, B cells and T cells produce memory cells. Throughout the lifeti… |
| 3 | Ebola virus `242#4` (128) — Prevention In December 2016, a study found the VSV-EBOV vaccine to be very effective (in t… | Tuberculosis `4760#2` (85) — There is a vaccine against some forms of tuberculosis. It is called the Bacillus Calmette–… |
| 4 | Lymphocyte `6034#1` (247) — T and B cells T cells (thymus cells) and B cells (bone cells) are the main cells of the ad… | Immunology `394#8` (66) — The distribution of vaccines and other immune system affecting cures can be considered ano… |
| 5 | Tuberculosis `4760#1` (191) — There is a vaccine against some forms of tuberculosis. It is called the Bacillus Calmette–… | Ebola virus `242#5` (551*) — 3. The virus then, goes on to attack spleen, kidneys and even the brain. The blood vessels… |


### q04 — what is the capital of Australia

**depth 0** — articles in both heads (Jaccard) 0.50; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Australia `27#0` (245) — Australia (officially called the Commonwealth of Australia) is a country and sovereign sta… | Australia `27#0` (118) — Australia (officially called the Commonwealth of Australia) is a country and sovereign sta… |
| 2 | Australia `27#2` (250) — Most of the Australian colonies, having been settled from Britain, became mostly independe… | Australia `27#6` (4745*) — Australia is a member of the United Nations and the Commonwealth of Nations. It is a parli… |
| 3 | Australia `27#3` (245) — Australia is a very large country, but much of the land is very dry, and the middle of the… | Australia `27#3` (70) — Australia is also known for its animals and rich wildlife. The national symbols of Austral… |
| 4 | Australia `27#1` (176) — Australia is also known for its animals and rich wildlife. The national symbols of Austral… | Australia `27#2` (50) — Australia is known for its mining (coal, iron, gold, diamonds and crystals), its productio… |
| 5 | Australia `27#19` (232) — Today Australia is a rich, peaceful and democratic country. But it still has problems. Aro… | Australia `27#1` (83) — 25 million people live in Australia, and about 85% of them live near the east coast. The c… |

**depth 10** — articles in both heads (Jaccard) 0.50; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Australia `27#0` (245) — Australia (officially called the Commonwealth of Australia) is a country and sovereign sta… | Australia `27#0` (118) — Australia (officially called the Commonwealth of Australia) is a country and sovereign sta… |
| 2 | Australia `27#2` (250) — Most of the Australian colonies, having been settled from Britain, became mostly independe… | Australia `27#6` (4745*) — Australia is a member of the United Nations and the Commonwealth of Nations. It is a parli… |
| 3 | Australia `27#3` (245) — Australia is a very large country, but much of the land is very dry, and the middle of the… | Australia `27#3` (70) — Australia is also known for its animals and rich wildlife. The national symbols of Austral… |
| 4 | Australia `27#1` (176) — Australia is also known for its animals and rich wildlife. The national symbols of Austral… | Australia `27#1` (83) — 25 million people live in Australia, and about 85% of them live near the east coast. The c… |
| 5 | Australia `27#19` (232) — Today Australia is a rich, peaceful and democratic country. But it still has problems. Aro… | Australia `27#2` (50) — Australia is known for its mining (coal, iron, gold, diamonds and crystals), its productio… |


### q05 — who painted the Mona Lisa

**depth 0** — articles in both heads (Jaccard) 0.75; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Mona Lisa `5163#0` (240) — Mona Lisa (also known as La Gioconda or La Joconde) is a 16th-century portrait. It was pai… | Mona Lisa `5163#0` (127) — Mona Lisa (also known as La Gioconda or La Joconde) is a 16th-century portrait. It was pai… |
| 2 | Mona Lisa `5163#1` (245) — Notes discovered in Heidelberg University Library which were written by Agostino Vespucci,… | Mona Lisa `5163#3` (76) — Lisa was the wife of Francesco del Giocondo a rich silk merchant, who lived in Florence. A… |
| 3 | Mona Lisa `5163#3` (165) — It was lost for two years, and everybody thought it would be lost forever. A worker at the… | Mona Lisa `5163#2` (101) — Giorgio Vasari, who was Leonardo's first biographer (a person who writes about the life of… |
| 4 | Leonardo da Vinci `4654#15` (168) — In about 1503 Leonardo began painting the portrait of a woman known as Mona Lisa, the most… | Mona Lisa `5163#4` (66) — According to this theory, at the time that Leonardo painted the portrait of his mother, wh… |
| 5 | Mona Lisa `5163#2` (155) — The Mona Lisa used to hang in the Chateau Fontainebleau and was then moved to the Palace o… | Mona Lisa `5163#6` (337*) — The painting was brought to France by Leonardo in 1516 and it was bought by Francis I of F… |

**depth 10** — articles in both heads (Jaccard) 0.75; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Mona Lisa `5163#0` (240) — Mona Lisa (also known as La Gioconda or La Joconde) is a 16th-century portrait. It was pai… | Mona Lisa `5163#0` (127) — Mona Lisa (also known as La Gioconda or La Joconde) is a 16th-century portrait. It was pai… |
| 2 | Mona Lisa `5163#1` (245) — Notes discovered in Heidelberg University Library which were written by Agostino Vespucci,… | Mona Lisa `5163#2` (101) — Giorgio Vasari, who was Leonardo's first biographer (a person who writes about the life of… |
| 3 | Leonardo da Vinci `4654#15` (168) — In about 1503 Leonardo began painting the portrait of a woman known as Mona Lisa, the most… | Mona Lisa `5163#3` (76) — Lisa was the wife of Francesco del Giocondo a rich silk merchant, who lived in Florence. A… |
| 4 | Mona Lisa `5163#3` (165) — It was lost for two years, and everybody thought it would be lost forever. A worker at the… | Mona Lisa `5163#4` (66) — According to this theory, at the time that Leonardo painted the portrait of his mother, wh… |
| 5 | Leonardo da Vinci `4654#16` (202) — The reason why the painting is so famous is that it seems to be full of mystery. Mona Lisa… | Mona Lisa `5163#5` (55) — Leonardo began painting the Mona Lisa in 1503 or 1504 in Florence, Italy. According to Da … |


### q06 — how does photosynthesis make oxygen

**depth 0** — articles in both heads (Jaccard) 1.00; top-1 article differs; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Photosynthesis `3518#1` (254) — Before photosynthesis, the Earth's atmosphere had almost no oxygen. Even without oxygen, s… | Oxygen `2949#1` (125) — Most living things use oxygen in respiration. Many molecules in living things have oxygen … |
| 2 | Photosynthesis `3518#0` (235) — Photosynthesis is how plants and some microorganisms make carbohydrates. It is an endother… | Photosynthesis `3518#7` (49) — Oxygen is produced as a result of photosynthesis and released into the atmosphere through … |
| 3 | Photosynthesis `3518#2` (240) — Glucose is used in respiration (to release energy in cells). It is stored in the form of s… | Photosynthesis `3518#4` (48) — 6 CO2(g) + 6 H2O + photons → C6H12O6(aq) + 6 O2(g) carbon dioxide + water + light energy →… |
| 4 | Photosynthesis `3518#5` (209) — Greenhouses must keep an optimum temperature for normal functioning of plants. Early evolu… | Photosynthesis `3518#1` (74) — Photosynthesis is vital for life on Earth. Before photosynthesis, Earth had no free oxygen… |
| 5 | Oxygen `2949#1` (183) — Most living things use oxygen in respiration. Many molecules in living things have oxygen … | Photosynthesis `3518#3` (72) — Before photosynthesis, the Earth's atmosphere had almost no oxygen. Even without oxygen, s… |

**depth 10** — articles in both heads (Jaccard) 1.00; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Photosynthesis `3518#0` (235) — Photosynthesis is how plants and some microorganisms make carbohydrates. It is an endother… | Photosynthesis `3518#7` (49) — Oxygen is produced as a result of photosynthesis and released into the atmosphere through … |
| 2 | Photosynthesis `3518#1` (254) — Before photosynthesis, the Earth's atmosphere had almost no oxygen. Even without oxygen, s… | Oxygen `2949#1` (125) — Most living things use oxygen in respiration. Many molecules in living things have oxygen … |
| 3 | Photosynthesis `3518#2` (240) — Glucose is used in respiration (to release energy in cells). It is stored in the form of s… | Photosynthesis `3518#1` (74) — Photosynthesis is vital for life on Earth. Before photosynthesis, Earth had no free oxygen… |
| 4 | Oxygen `2949#1` (183) — Most living things use oxygen in respiration. Many molecules in living things have oxygen … | Photosynthesis `3518#3` (72) — Before photosynthesis, the Earth's atmosphere had almost no oxygen. Even without oxygen, s… |
| 5 | Photosynthesis `3518#5` (209) — Greenhouses must keep an optimum temperature for normal functioning of plants. Early evolu… | Photosynthesis `3518#0` (80) — Photosynthesis is how plants and some microorganisms make carbohydrates. It is an endother… |


### q07 — what causes earthquakes

**depth 0** — articles in both heads (Jaccard) 0.40; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Earthquake `2081#2` (221) — These belts are along the edges of tectonic plates. The tectonic plates push on each other… | Earthquake `2081#7` (931*) — Causes of an earthquake Earthquakes are caused by tectonic movements in the Earth's crust.… |
| 2 | Earthquake `2081#3` (236) — Often the causes are not known, except in the most general terms. An example is the larges… | Earthquake `2081#0` (109) — An earthquake is when Earth's tectonic plates shake and move Earth's surface. Strong earth… |
| 3 | Earthquake `2081#0` (172) — An earthquake is when Earth's tectonic plates shake and move Earth's surface. Strong earth… | Earthquake `2081#1` (66) — People who study earthquakes are called seismologists. Many earthquakes can occur in a sma… |
| 4 | Earthquake `2081#4` (222) — An aftershock is an earthquake that happens after a previous earthquake, the mainshock. An… | Earthquake `2081#4` (56) — No one can tell when an earthquake will happen. But we know where earthquakes are likely t… |
| 5 | Earthquake `2081#1` (248) — The effect of an earthquake can be measured with a seismometer. A seismometer detects the … | Earthquake `2081#6` (51) — History Earthquakes sometimes hit cities and kill hundreds or thousands of people. Most ea… |

**depth 10** — articles in both heads (Jaccard) 0.40; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Earthquake `2081#2` (221) — These belts are along the edges of tectonic plates. The tectonic plates push on each other… | Earthquake `2081#7` (931*) — Causes of an earthquake Earthquakes are caused by tectonic movements in the Earth's crust.… |
| 2 | Earthquake `2081#0` (172) — An earthquake is when Earth's tectonic plates shake and move Earth's surface. Strong earth… | Earthquake `2081#0` (109) — An earthquake is when Earth's tectonic plates shake and move Earth's surface. Strong earth… |
| 3 | Earthquake `2081#3` (236) — Often the causes are not known, except in the most general terms. An example is the larges… | Earthquake `2081#1` (66) — People who study earthquakes are called seismologists. Many earthquakes can occur in a sma… |
| 4 | Earthquake `2081#1` (248) — The effect of an earthquake can be measured with a seismometer. A seismometer detects the … | Earthquake `2081#4` (56) — No one can tell when an earthquake will happen. But we know where earthquakes are likely t… |
| 5 | Earthquake `2081#5` (223) — A tsunami is a chain of fast moving waves in the ocean caused by powerful earthquakes. The… | Earthquake `2081#6` (51) — History Earthquakes sometimes hit cities and kill hundreds or thousands of people. Most ea… |


### q08 — when did the Berlin Wall fall

**depth 0** — articles in both heads (Jaccard) 0.33; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Berlin `2922#0` (254) — Berlin (; ) is the capital city of Germany. It is the largest city in the European Union b… | Berlin `2922#1` (115) — Berlin is an important city for the history of Germany. The King of Prussia and the Empero… |
| 2 | Wall `1996#0` (248) — A wall is a vertical dividing surface. It divides space in buildings into rooms or protect… | Wall `1996#1` (64) — The term "the Wall" usually referred to the Berlin Wall, built during the Cold War, which … |
| 3 | Berlin `2922#2` (249) — There is now West Berlin and East Berlin. 1961: The Berlin Wall was built by the communist… | Berlin `2922#3` (1872*) — History 1244: The first writings about a place called Berlin. 1451: The Hohenzollern famil… |
| 4 | Berlin `2922#1` (251) — Berlin is a world city of culture, start ups, politics, media and science. There are a lot… | Berlin `2922#0` (142) — Berlin (; ) is the capital city of Germany. It is the largest city in the European Union b… |
| 5 | Cold War `1949#11` (106) — After the fall of the Berlin Wall in 1989 and without communist rule holding together the … | Berlin `2922#2` (62) — Berlin is a world city of culture, start ups, politics, media and science. There are a lot… |

**depth 10** — articles in both heads (Jaccard) 0.33; top-1 article differs; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Berlin `2922#2` (249) — There is now West Berlin and East Berlin. 1961: The Berlin Wall was built by the communist… | Wall `1996#1` (64) — The term "the Wall" usually referred to the Berlin Wall, built during the Cold War, which … |
| 2 | Wall `1996#0` (248) — A wall is a vertical dividing surface. It divides space in buildings into rooms or protect… | Berlin `2922#1` (115) — Berlin is an important city for the history of Germany. The King of Prussia and the Empero… |
| 3 | Cold War `1949#11` (106) — After the fall of the Berlin Wall in 1989 and without communist rule holding together the … | Berlin `2922#3` (1872*) — History 1244: The first writings about a place called Berlin. 1451: The Hohenzollern famil… |
| 4 | Berlin `2922#0` (254) — Berlin (; ) is the capital city of Germany. It is the largest city in the European Union b… | Berlin `2922#0` (142) — Berlin (; ) is the capital city of Germany. It is the largest city in the European Union b… |
| 5 | Cold War `1949#5` (205) — From April 1948 to May 1949, the Soviets blockaded West Berlin to prevent the city from us… | Berlin `2922#2` (62) — Berlin is a world city of culture, start ups, politics, media and science. There are a lot… |


### q09 — how many players are on a football team

**depth 0** — articles in both heads (Jaccard) 0.75; top-1 article differs; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Football `3039#1` (236) — Some elements of football have been seen in many countries, dating back to the second and … | Association football `2062#2` (69) — Each team has 11 players on the field. One of these players is the goalkeeper, the only pl… |
| 2 | Association football `2062#3` (150) — If a player hits the ball out of play at their end of the field, the other team kicks the … | Football `3039#5` (77) — Every year there is a Football Club Competition called as Champions League. All the qualif… |
| 3 | Football `3039#0` (215) — Football is a word which could mean one of several sports. The best-known type of football… | Association football `2062#8` (1145*) — Before this rule was added, players would often stand next to their opponents' goal and sc… |
| 4 | List of English football teams `3052#0` (67) — This is a list of English football teams in the Premier League (the top or 1st level leagu… | List of English football teams `3052#1` (16) — A-Z order Lists of football teams team |
| 5 | Association football `2062#1` (253) — Each team has 11 players on the field. One of these players is the goalkeeper, the only pl… | Football `3039#2` (111) — Football is played using a ball, also called a 'football', that is usually shaped like a s… |

**depth 10** — articles in both heads (Jaccard) 0.75; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Association football `2062#1` (253) — Each team has 11 players on the field. One of these players is the goalkeeper, the only pl… | Association football `2062#2` (69) — Each team has 11 players on the field. One of these players is the goalkeeper, the only pl… |
| 2 | Association football `2062#3` (150) — If a player hits the ball out of play at their end of the field, the other team kicks the … | Football `3039#5` (77) — Every year there is a Football Club Competition called as Champions League. All the qualif… |
| 3 | Football `3039#1` (236) — Some elements of football have been seen in many countries, dating back to the second and … | Association football `2062#8` (1145*) — Before this rule was added, players would often stand next to their opponents' goal and sc… |
| 4 | Football `3039#0` (215) — Football is a word which could mean one of several sports. The best-known type of football… | List of English football teams `3052#1` (16) — A-Z order Lists of football teams team |
| 5 | List of English football teams `3052#0` (67) — This is a list of English football teams in the Premier League (the top or 1st level leagu… | Football `3039#2` (111) — Football is played using a ball, also called a 'football', that is usually shaped like a s… |


### q10 — what is the longest river in Africa

**depth 0** — articles in both heads (Jaccard) 0.71; top-1 article differs; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | River `673#8` (222) — Amazon River in South America is a very wide tropical river flowing through the Amazon Jun… | Africa `1942#0` (79) — Africa is the second largest continent in the world. It makes up about a fifth of the worl… |
| 2 | Africa `1942#0` (233) — Africa is the second largest continent in the world. It makes up about a fifth of the worl… | Africa `1942#7` (1057*) — Egypt and Sudan were never formally made a part of any European colonial empire. However, … |
| 3 | Africa `1942#3` (256) — Running north-east to the south is the East African Great Rift Valley. This has mountains,… | Mississippi River `3349#0` (188) — The Mississippi River is a river in the United States. It is the 11th longest river in the… |
| 4 | Africa `1942#6` (242) — Countries with significant African descendents outside Africa: Haiti: 98% Saint Kitts and … | South America `1989#6` (163) — The amazon rain forest is a moist grassy land where many wild animals live and contains th… |
| 5 | Africa `1942#5` (174) — Extensive human rights abuses still occur in several parts of Africa, often under the over… | Africa `1942#1` (67) — History The history of Africa begins with the first modern human beings and continues to i… |

**depth 10** — articles in both heads (Jaccard) 0.71; top-1 article differs; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | River `673#8` (222) — Amazon River in South America is a very wide tropical river flowing through the Amazon Jun… | South America `1989#6` (163) — The amazon rain forest is a moist grassy land where many wild animals live and contains th… |
| 2 | Africa `1942#0` (233) — Africa is the second largest continent in the world. It makes up about a fifth of the worl… | Africa `1942#0` (79) — Africa is the second largest continent in the world. It makes up about a fifth of the worl… |
| 3 | Africa `1942#3` (256) — Running north-east to the south is the East African Great Rift Valley. This has mountains,… | Mississippi River `3349#0` (188) — The Mississippi River is a river in the United States. It is the 11th longest river in the… |
| 4 | Mississippi River `3349#0` (237) — The Mississippi River is a river in the United States. It is the 11th longest river in the… | Africa `1942#7` (1057*) — Egypt and Sudan were never formally made a part of any European colonial empire. However, … |
| 5 | South Africa `3528#3` (217) — South Africa is found at the southernmost region of Africa, with a long coastline that rea… | Africa `1942#1` (67) — History The history of Africa begins with the first modern human beings and continues to i… |


### q11 — who invented the telephone

**depth 0** — articles in both heads (Jaccard) 0.50; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Telephone `6222#0` (168) — A telephone, also known as a phone, is a communication tool. People use it to talk with pe… | Telephone `6222#0` (164) — A telephone, also known as a phone, is a communication tool. People use it to talk with pe… |
| 2 | Alexander Graham Bell `4579#0` (249) — Alexander Graham Bell (March 3, 1847 - August 2, 1922) was a Scottish-born British-Canadia… | Telephone `6222#2` (56) — Most countries have a telephone network. The telephones in one place are connected to a te… |
| 3 | Telephone `6222#1` (224) — There are many different types of telephones. A telephone that can be carried around is ca… | Telephone `6222#1` (175) — Types of telephones There are many different types of telephones. A telephone that can be … |
| 4 | Alexander Graham Bell `4579#6` (216) — The first long distance telephone call was made on August 10, 1876 by Bell from the family… | Alexander Graham Bell `4579#0` (142) — Alexander Graham Bell (March 3, 1847 - August 2, 1922) was a Scottish-born British-Canadia… |
| 5 | Alexander Graham Bell `4579#3` (142) — Inventions Bell's genius is seen in part by the eighteen patents granted in his name alone… | Leonardo da Vinci `4654#2` (71) — Leonardo often thought of new inventions. He kept notebooks with notes and drawings of the… |

**depth 10** — articles in both heads (Jaccard) 0.50; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Telephone `6222#0` (168) — A telephone, also known as a phone, is a communication tool. People use it to talk with pe… | Telephone `6222#0` (164) — A telephone, also known as a phone, is a communication tool. People use it to talk with pe… |
| 2 | Alexander Graham Bell `4579#0` (249) — Alexander Graham Bell (March 3, 1847 - August 2, 1922) was a Scottish-born British-Canadia… | Alexander Graham Bell `4579#0` (142) — Alexander Graham Bell (March 3, 1847 - August 2, 1922) was a Scottish-born British-Canadia… |
| 3 | Alexander Graham Bell `4579#6` (216) — The first long distance telephone call was made on August 10, 1876 by Bell from the family… | Telephone `6222#1` (175) — Types of telephones There are many different types of telephones. A telephone that can be … |
| 4 | Alexander Graham Bell `4579#3` (142) — Inventions Bell's genius is seen in part by the eighteen patents granted in his name alone… | Leonardo da Vinci `4654#2` (71) — Leonardo often thought of new inventions. He kept notebooks with notes and drawings of the… |
| 5 | Telephone `6222#1` (224) — There are many different types of telephones. A telephone that can be carried around is ca… | Alexander Graham Bell `4579#5` (993*) — Alexander Graham Bell soon became famous in the United States for this important work. He … |


### q12 — what do bees make honey from

**depth 0** — articles in both heads (Jaccard) 0.43; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Bee `5896#0` (206) — Bees are flying insects of the Hymenoptera, which also includes ants, wasps and sawflies. … | Bee `5896#0` (222) — Bees are flying insects of the Hymenoptera, which also includes ants, wasps and sawflies. … |
| 2 | Bee `5896#3` (241) — Social bees Some bees are eusocial insects; this means they live in organized groups calle… | Honeycomb `4603#0` (110) — A honeycomb is a container made by bees out of wax that they produce. The bees make a hone… |
| 3 | Honeycomb `4603#0` (183) — A honeycomb is a container made by bees out of wax that they produce. The bees make a hone… | Bee `5896#1` (77) — Bees are different because they are specialized as pollination agents. Their body and beha… |
| 4 | Bee `5896#1` (185) — Evolution Flowers were pollinated by insects such as beetles long before bees first appear… | Bee `5896#4` (527*) — Bee bodies Like other insects, the body of a bee can be divided into three parts: the head… |
| 5 | Bee `5896#4` (140) — Because a male has only one copy of each gene, his daughters (which are diploid, with two … | Honeycomb `4603#2` (59) — A worker bee grows wax in its abdomen. The wax appears as little spots. The bee pulls it o… |

**depth 10** — articles in both heads (Jaccard) 0.43; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Honeycomb `4603#0` (183) — A honeycomb is a container made by bees out of wax that they produce. The bees make a hone… | Honeycomb `4603#0` (110) — A honeycomb is a container made by bees out of wax that they produce. The bees make a hone… |
| 2 | Bee `5896#0` (206) — Bees are flying insects of the Hymenoptera, which also includes ants, wasps and sawflies. … | Bee `5896#0` (222) — Bees are flying insects of the Hymenoptera, which also includes ants, wasps and sawflies. … |
| 3 | Bee `5896#3` (241) — Social bees Some bees are eusocial insects; this means they live in organized groups calle… | Beekeeping `80#0` (108) — Beekeeping or apiculture is the farming of honeybees. Uses The keeping of bees is usually,… |
| 4 | Bee `5896#1` (185) — Evolution Flowers were pollinated by insects such as beetles long before bees first appear… | Honeycomb `4603#2` (59) — A worker bee grows wax in its abdomen. The wax appears as little spots. The bee pulls it o… |
| 5 | Beekeeping `80#0` (192) — Beekeeping or apiculture is the farming of honeybees. Uses The keeping of bees is usually,… | Bee `5896#1` (77) — Bees are different because they are specialized as pollination agents. Their body and beha… |


### q13 — how far is the Moon from Earth

**depth 0** — articles in both heads (Jaccard) 0.57; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Earth `219#1` (255) — Earth is one of the eight planets in the Solar System. There are also thousands of small b… | Earth `219#5` (114) — The Moon goes around Earth at an average distance of . It is locked to Earth, so that it a… |
| 2 | Phoebe (moon) `5093#0` (177) — Phoebe is a moon which goes around (orbits) the planet called Saturn. It takes eighteen mo… | Phoebe (moon) `5093#1` (94) — There are many craters on Phoebe. These are from asteroids and other things crashing into … |
| 3 | Phases of the Moon `3540#0` (213) — The phases of the Moon are the different ways the Moon looks from Earth over about a month… | Earth `219#3` (34) — Earth is about away from the Sun (this distance is called an "Astronomical Unit"). It move… |
| 4 | Earth `219#2` (147) — Earth and the other planets formed about 4.6 billion years ago. Their origin was quite dif… | Phoebe (moon) `5093#0` (85) — Phoebe is a moon which goes around (orbits) the planet called Saturn. It takes eighteen mo… |
| 5 | Earth `219#11` (139) — Surface The outside of the Earth is not even. There are high places called mountains, and … | Phases of the Moon `3540#3` (23) — Related pages Extraterrestrial sky Phases of the Earth Lunar eclipse Solar eclipse Referen… |

**depth 10** — articles in both heads (Jaccard) 0.57; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Earth `219#1` (255) — Earth is one of the eight planets in the Solar System. There are also thousands of small b… | Earth `219#5` (114) — The Moon goes around Earth at an average distance of . It is locked to Earth, so that it a… |
| 2 | Phoebe (moon) `5093#0` (177) — Phoebe is a moon which goes around (orbits) the planet called Saturn. It takes eighteen mo… | Phoebe (moon) `5093#1` (94) — There are many craters on Phoebe. These are from asteroids and other things crashing into … |
| 3 | Phases of the Moon `3540#0` (213) — The phases of the Moon are the different ways the Moon looks from Earth over about a month… | Earth `219#3` (34) — Earth is about away from the Sun (this distance is called an "Astronomical Unit"). It move… |
| 4 | Speed of light `4322#4` (189) — Rømer Ole Christensen Rømer used an astronomical measurement to make the first quantitativ… | Phoebe (moon) `5093#0` (85) — Phoebe is a moon which goes around (orbits) the planet called Saturn. It takes eighteen mo… |
| 5 | Earth `219#2` (147) — Earth and the other planets formed about 4.6 billion years ago. Their origin was quite dif… | Light year `2143#2` (99) — Similar distance measurements Light minute - The distance that light travels in one minute… |


### q14 — what language is spoken in Brazil

**depth 0** — articles in both heads (Jaccard) 0.67; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Brazil `104#2` (194) — During the 1960s, the military leader Castelo Branco overthrew the government and created … | Brazil `104#6` (731*) — Since then, the country has become more democratic, but some people feel that there are st… |
| 2 | Brazil `104#3` (211) — Other people in Brazil speak their ancestors' languages like Italian, Japanese, Polish, Uk… | Brazil `104#0` (72) — Brazil (officially called Federative Republic of Brazil; how to say: ) is a country in Sou… |
| 3 | Brazil `104#5` (191) — Culture Brazil is the largest country in South America and the fifth-largest in the world.… | Spanish language `6432#8` (297*) — In the other Romance languages spoken on the Iberian Peninsula, such as Galician, Catalan,… |
| 4 | Brazil `104#4` (192) — Brazil is divided into 26 states plus the Federal District in five regions (north, south, … | Brazil `104#2` (91) — Pedro Álvares Cabral was the first European to see Brazil. He saw it in 1500. He was from … |
| 5 | Brazil `104#0` (218) — Brazil (officially called Federative Republic of Brazil; how to say: ) is a country in Sou… | Brazil `104#1` (61) — History The first people to come to Brazil came around 9,000 B.C. That group of indigenous… |

**depth 10** — articles in both heads (Jaccard) 0.67; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Brazil `104#3` (211) — Other people in Brazil speak their ancestors' languages like Italian, Japanese, Polish, Uk… | Brazil `104#6` (731*) — Since then, the country has become more democratic, but some people feel that there are st… |
| 2 | Brazil `104#2` (194) — During the 1960s, the military leader Castelo Branco overthrew the government and created … | Spanish language `6432#8` (297*) — In the other Romance languages spoken on the Iberian Peninsula, such as Galician, Catalan,… |
| 3 | Brazil `104#5` (191) — Culture Brazil is the largest country in South America and the fifth-largest in the world.… | Brazil `104#0` (72) — Brazil (officially called Federative Republic of Brazil; how to say: ) is a country in Sou… |
| 4 | Spanish language `6432#2` (173) — The Spanish word for Spanish is "español", and the Spanish word for Castilian is "castella… | Brazil `104#2` (91) — Pedro Álvares Cabral was the first European to see Brazil. He saw it in 1500. He was from … |
| 5 | Spanish language `6432#1` (245) — In the United States of America and Belize, most people use English, but Spanish is the se… | Brazil `104#1` (61) — History The first people to come to Brazil came around 9,000 B.C. That group of indigenous… |


### q15 — why do leaves change colour in autumn

**depth 0** — articles in both heads (Jaccard) 0.40; top-1 article differs; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Tree `847#4` (192) — Leaves The leaves of a tree are held by the branches. Leaves are usually held at the ends … | Colour `2083#2` (93) — Colours are sometimes added to food. Food colouring is used to colour food, but some foods… |
| 2 | Variegated leaf `3900#0` (205) — A variegated leaf is a type of leaf. Uses A variegated leaf is useful in conducting experi… | Variegated leaf `3900#1` (154) — Experiments The easiest of these experiments is to stain the places where starch is produc… |
| 3 | Bird `3707#5` (181) — Many birds are brown, green or grey. These colours make a bird harder to be seen: they cam… | Colour `2083#0` (79) — Colour is a property of light. In American English, the name is "color" without the "u." I… |
| 4 | Mars `515#4` (224) — Mars has two permanent polar ice caps. During a pole's winter, it lies in continuous darkn… | Tree `847#4` (3191*) — In areas with seasonal climate, wood produced at different times of the year may alternate… |
| 5 | Colour `2083#0` (249) — Colour is a property of light. In American English, the name is "color" without the "u." I… | Colour `2083#1` (107) — Primary colours can be mixed to make other colours. Red, yellow, and blue are the three tr… |

**depth 10** — articles in both heads (Jaccard) 0.40; top-1 article differs; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Tree `847#4` (192) — Leaves The leaves of a tree are held by the branches. Leaves are usually held at the ends … | Variegated leaf `3900#1` (154) — Experiments The easiest of these experiments is to stain the places where starch is produc… |
| 2 | Variegated leaf `3900#0` (205) — A variegated leaf is a type of leaf. Uses A variegated leaf is useful in conducting experi… | Tree `847#4` (3191*) — In areas with seasonal climate, wood produced at different times of the year may alternate… |
| 3 | Bird `3707#5` (181) — Many birds are brown, green or grey. These colours make a bird harder to be seen: they cam… | Colour `2083#2` (93) — Colours are sometimes added to food. Food colouring is used to colour food, but some foods… |
| 4 | Tree `847#1` (194) — The leaves of a tree are green most of the time, but they can come in many colors, shapes … | Colour `2083#0` (79) — Colour is a property of light. In American English, the name is "color" without the "u." I… |
| 5 | Mars `515#4` (224) — Mars has two permanent polar ice caps. During a pole's winter, it lies in continuous darkn… | November `530#3` (62) — In the Northern Hemisphere, November is an Autumn (Fall) month, and the further north in t… |


### q16 — what is a black hole

**depth 0** — articles in both heads (Jaccard) 0.25; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Black hole `3506#11` (179) — The properties are special, because all of them can be measured from outside the black hol… | Black hole `3506#0` (152) — A black hole is a region of space-time from which nothing, not even light, can escape. Acc… |
| 2 | Black hole `3506#8` (201) — At the middle of a black hole, there is a gravitational center called a singularity. It is… | Black hole `3506#5` (2062*) — History In 1783, an English clergyman named John Michell wrote that it might be possible f… |
| 3 | Black hole `3506#0` (130) — A black hole is a region of space-time from which nothing, not even light, can escape. Acc… | Black hole `3506#1` (84) — The place where there is a black hole can be found by tracking the movement of stars that … |
| 4 | Black hole `3506#3` (183) — In 1916 Albert Einstein wrote an explanation of gravity called general relativity. Mass ca… | Black hole `3506#3` (7) — /ref> |
| 5 | Black hole `3506#7` (239) — Huge central masses (106 to 109 solar masses) have been measured in quasars. Several dozen… | Black hole `3506#2` (49) — <ref>Wald, Robert M. 1992. Space, time, and gravity: the theory of the Big Bang and black … |

**depth 10** — articles in both heads (Jaccard) 0.25; top-1 article same; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Black hole `3506#0` (130) — A black hole is a region of space-time from which nothing, not even light, can escape. Acc… | Black hole `3506#0` (152) — A black hole is a region of space-time from which nothing, not even light, can escape. Acc… |
| 2 | Black hole `3506#8` (201) — At the middle of a black hole, there is a gravitational center called a singularity. It is… | Black hole `3506#1` (84) — The place where there is a black hole can be found by tracking the movement of stars that … |
| 3 | Black hole `3506#11` (179) — The properties are special, because all of them can be measured from outside the black hol… | Black hole `3506#3` (7) — /ref> |
| 4 | Black hole `3506#3` (183) — In 1916 Albert Einstein wrote an explanation of gravity called general relativity. Mass ca… | Black hole `3506#2` (49) — <ref>Wald, Robert M. 1992. Space, time, and gravity: the theory of the Big Bang and black … |
| 5 | A Brief History of Time `4570#10` (253) — Black holes are stars that have collapsed into one very small point. This small point is c… | Black hole `3506#5` (2062*) — History In 1783, an English clergyman named John Michell wrote that it might be possible f… |


### q17 — who wrote Romeo and Juliet

**depth 0** — articles in both heads (Jaccard) 0.29; top-1 article differs; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | England `3047#10` (169) — William Shakespeare was an English playwright. He wrote plays in the late 16th century. So… | Socrates `5128#5` (66) — Another of Socrates' students, Xenophon also wrote about Socrates. Aristophanes, a person … |
| 2 | Jane Austen `5133#8` (249) — Alexander, Christine and Juliet McMaster, eds. The Child Writer from Austen to Woolf. Camb… | Plato `3684#0` (123) — Plato was one of the most important classical Greek philosophers. He lived from 427 BC to … |
| 3 | Socrates `5128#2` (228) — Another of Socrates' students, Xenophon also wrote about Socrates. Aristophanes, a person … | Jane Austen `5133#0` (136) — Jane Austen (16 December 1775 – 18 July 1817) was an English novelist. She wrote many book… |
| 4 | Johann Sebastian Bach `4463#0` (216) — Johann Sebastian Bach (31 March 1685 in Eisenach – 28 July 1750 in Leipzig; pronounced BAH… | Johann Sebastian Bach `4463#0` (216) — Johann Sebastian Bach (31 March 1685 in Eisenach – 28 July 1750 in Leipzig; pronounced BAH… |
| 5 | Johann Wolfgang von Goethe `5125#1` (244) — Education In his youth, he learned Greek, Latin and French. He studied law in Leipzig from… | Socrates `5128#4` (87) — The life of Socrates Socrates never wrote anything. All of what we know about Socrates is … |

**depth 10** — articles in both heads (Jaccard) 0.29; top-1 article differs; chonky head passages over the window: 1/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | England `3047#10` (169) — William Shakespeare was an English playwright. He wrote plays in the late 16th century. So… | Jane Austen `5133#0` (136) — Jane Austen (16 December 1775 – 18 July 1817) was an English novelist. She wrote many book… |
| 2 | Jane Austen `5133#8` (249) — Alexander, Christine and Juliet McMaster, eds. The Child Writer from Austen to Woolf. Camb… | Socrates `5128#5` (66) — Another of Socrates' students, Xenophon also wrote about Socrates. Aristophanes, a person … |
| 3 | Socrates `5128#2` (228) — Another of Socrates' students, Xenophon also wrote about Socrates. Aristophanes, a person … | Jane Austen `5133#2` (193) — Jane Austen was very modest about her own genius. She once famously described her work as … |
| 4 | United Kingdom `856#17` (222) — Britain continued to be the biggest manufacturing economy in the world until 1908 and the … | Wolfgang Amadeus Mozart `4462#3` (119) — Mozart wrote more than 600 musical works, all of the very highest quality. His works inclu… |
| 5 | Johann Sebastian Bach `4463#0` (216) — Johann Sebastian Bach (31 March 1685 in Eisenach – 28 July 1750 in Leipzig; pronounced BAH… | Theatre `819#5` (626*) — Middle Ages In the Middle Ages, the Catholic Church began to use theatre as a way of telli… |


### q18 — how is chess played

**depth 0** — articles in both heads (Jaccard) 0.20; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Chess `3259#0` (240) — Chess is a board game for two players. It is played on a square board, made up of 64 small… | Chess `3259#0` (239) — Chess is a board game for two players. It is played on a square board, made up of 64 small… |
| 2 | Chess `3259#25` (119) — Chessgames Chessgames.com runs an online database of games. It is mostly free; full access… | Chess `3259#1` (102) — History Most historians agree that the game of chess was first played in northern India du… |
| 3 | Chess `3259#2` (245) — The game changed greatly between about 1470 to 1495. The rules of the older game were chan… | Chess `3259#3` (5145*) — In the 13th century, a Spanish manuscript called Libro de los Juegos described the games o… |
| 4 | Chess `3259#11` (97) — Chess clocks Competitive games of chess must be played with special chess clocks which tim… | Chess `3259#2` (90) — The earliest written evidence of chess is found in three romances (epic stories) written i… |
| 5 | Chess `3259#26` (225) — Further reading Burgess, Graham and John Nunn 1998. The mammoth book of the world's greate… | K `5728#2` (34) — in chess, K is a notation symbol for the king piece In a deck of playing cards, the letter… |

**depth 10** — articles in both heads (Jaccard) 0.20; top-1 article same; chonky head passages over the window: 2/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Chess `3259#0` (240) — Chess is a board game for two players. It is played on a square board, made up of 64 small… | Chess `3259#0` (239) — Chess is a board game for two players. It is played on a square board, made up of 64 small… |
| 2 | Chess `3259#11` (97) — Chess clocks Competitive games of chess must be played with special chess clocks which tim… | Chess `3259#1` (102) — History Most historians agree that the game of chess was first played in northern India du… |
| 3 | Chess `3259#2` (245) — The game changed greatly between about 1470 to 1495. The rules of the older game were chan… | Chess `3259#3` (5145*) — In the 13th century, a Spanish manuscript called Libro de los Juegos described the games o… |
| 4 | Chess `3259#25` (119) — Chessgames Chessgames.com runs an online database of games. It is mostly free; full access… | Chess `3259#2` (90) — The earliest written evidence of chess is found in three romances (epic stories) written i… |
| 5 | Chess `3259#23` (220) — Basic checkmates Basic checkmates are positions in which one side has only a king and the … | Combinatorial game theory `3692#4` (418*) — In the theory, there are two players called left and right. A game is something that allow… |


### q19 — what is the boiling point of water

**depth 0** — articles in both heads (Jaccard) 0.57; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Boil `93#0` (36) — Boil might mean: Boiling, heating a liquid to the point where it turns into gas Boil, a ty… | Boil `93#0` (36) — Boil might mean: Boiling, heating a liquid to the point where it turns into gas Boil, a ty… |
| 2 | International System of Units `3222#5` (174) — |- !kelvin |style="text-align:center" |K |style="text-align:center" |Θ |thermodynamictempe… | Water vapor `4057#0` (131) — Water vapor is water that is in the form of a vapor, or gas. It is a part of the water cyc… |
| 3 | Water vapor `4057#0` (182) — Water vapor is water that is in the form of a vapor, or gas. It is a part of the water cyc… | Evaporation `4059#4` (53) — Liquid with high boiling points (those that boil at very high temperatures) tend to evapor… |
| 4 | Evaporation `4059#1` (102) — The rate of evaporation depends on the liquid's exposed surface area (faster when increase… | Water `939#2` (131) — Physical chemistry of water Water is a fluid. Water is the only chemical substance on Eart… |
| 5 | Water `939#1` (215) — Physical chemistry of water Water is a fluid. Water is the only chemical substance on Eart… | Water `939#1` (91) — Lakes, oceans, seas, and rivers are made of water. Precipitation is water that falls from … |

**depth 10** — articles in both heads (Jaccard) 0.57; top-1 article differs; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | International System of Units `3222#5` (174) — |- !kelvin |style="text-align:center" |K |style="text-align:center" |Θ |thermodynamictempe… | Water vapor `4057#0` (131) — Water vapor is water that is in the form of a vapor, or gas. It is a part of the water cyc… |
| 2 | Water vapor `4057#0` (182) — Water vapor is water that is in the form of a vapor, or gas. It is a part of the water cyc… | Boil `93#0` (36) — Boil might mean: Boiling, heating a liquid to the point where it turns into gas Boil, a ty… |
| 3 | Boil `93#0` (36) — Boil might mean: Boiling, heating a liquid to the point where it turns into gas Boil, a ty… | Evaporation `4059#4` (53) — Liquid with high boiling points (those that boil at very high temperatures) tend to evapor… |
| 4 | Evaporation `4059#1` (102) — The rate of evaporation depends on the liquid's exposed surface area (faster when increase… | Evaporation `4059#1` (100) — Differences between evaporation and boiling During evaporation only the molecules near the… |
| 5 | States of matter `3325#3` (215) — When a solid becomes a liquid, it is called melting. When a liquid becomes a solid, it is … | Water `939#2` (131) — Physical chemistry of water Water is a fluid. Water is the only chemical substance on Eart… |


### q20 — which animal is the largest on Earth

**depth 0** — articles in both heads (Jaccard) 0.62; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Animal `62#1` (186) — Animals can mainly be divided into two main groups: the invertebrates and the vertebrates.… | Animal `62#2` (107) — Animals can mainly be divided into two main groups: the invertebrates and the vertebrates.… |
| 2 | Animal `62#2` (183) — The environments animals live in vary greatly. By the process of evolution, animals adapt … | Animal `62#5` (110) — The fossil record of animals goes back about 600 million years to the Ediacaran period, or… |
| 3 | Animal `62#0` (245) — Animals (or Metazoa) are living creatures with many cells. Animals get their energy from o… | Insect `3750#0` (217) — Insects are a class in the phylum Arthropoda. They are small terrestrial invertebrates whi… |
| 4 | Vertebrate `5899#1` (252) — Distinctions Vertebrates dominate amongst the animals in virtually all environments. They … | Vertebrate `5899#3` (61) — Distinctions Vertebrates dominate amongst the animals in virtually all environments. They … |
| 5 | Insect `3750#0` (219) — Insects are a class in the phylum Arthropoda. They are small terrestrial invertebrates whi… | Animal `62#4` (76) — The environments animals live in vary greatly. By the process of evolution, animals adapt … |

**depth 10** — articles in both heads (Jaccard) 0.62; top-1 article same; chonky head passages over the window: 0/10

| # | contract chunker | chonky |
|---|---|---|
| 1 | Insect `3750#0` (219) — Insects are a class in the phylum Arthropoda. They are small terrestrial invertebrates whi… | Insect `3750#0` (217) — Insects are a class in the phylum Arthropoda. They are small terrestrial invertebrates whi… |
| 2 | Vertebrate `5899#1` (252) — Distinctions Vertebrates dominate amongst the animals in virtually all environments. They … | Vertebrate `5899#3` (61) — Distinctions Vertebrates dominate amongst the animals in virtually all environments. They … |
| 3 | Antarctica `1976#5` (213) — Largest land animal The largest animal in Antarctica that lives entirely on land is a wing… | Animal `62#2` (107) — Animals can mainly be divided into two main groups: the invertebrates and the vertebrates.… |
| 4 | Animal `62#1` (186) — Animals can mainly be divided into two main groups: the invertebrates and the vertebrates.… | Animal `62#5` (110) — The fossil record of animals goes back about 600 million years to the Ediacaran period, or… |
| 5 | Dinosaur `4455#10` (224) — The largest dinosaurs were herbivores (plant-eaters), such as Apatosaurus and Brachiosauru… | Lake `4031#2` (47) — The largest freshwater lake of South America is Lake Titicaca, which is also the highest b… |

