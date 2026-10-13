Feature: Retain alternate downloads of a tagged song

  Scenario: Different filenames and formats with the same title and artist retain alternatives
    Given "downloads/a-original/song.ncm" is a copy of the NCM fixture "standard.ncm"
    And "downloads/z-lossless/another-name.ncm" is a copy of the NCM fixture "flac.ncm"
    And "downloads/plain/renamed.mp3" is a copy of the fixture "same-song.mp3"

    When I import the folder "downloads"
    Then 3 tracks are added
    And 0 duplicates are skipped
    And 2 collisions are retained
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title       | audio         |
      | Fixture NCM | silence.mp3   |
      | Fixture NCM | tone.flac     |
      | Fixture NCM | same-song.mp3 |
    And the library has 1 browsing group

  Scenario: An NCM imported after restarting retains its own audio and cover
    Given "first/download.mp3" is a copy of the fixture "same-song.mp3"
    And "later/renamed.ncm" is a copy of the NCM fixture "standard.ncm"

    When I import the folder "first"
    Then 1 track is added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains only "Fixture NCM" with audio "same-song.mp3"

    When I reopen the library
    And I import the folder "later"
    Then 1 track is added
    And 0 duplicates are skipped
    And 1 collision is retained
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title       | audio         |
      | Fixture NCM | same-song.mp3 |
      | Fixture NCM | silence.mp3   |
    And the library has 1 browsing group

  Scenario: Different artists sharing a title and filename remain separate songs
    Given "downloads/artist-one/song.mp3" is a copy of the fixture "same-song.mp3"
    And "downloads/artist-two/song.mp3" is a copy of the fixture "other-artist.mp3"

    When I import the folder "downloads"
    Then 2 tracks are added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title       | audio            |
      | Fixture NCM | same-song.mp3    |
      | Fixture NCM | other-artist.mp3 |

  Scenario: Missing tags never turn the same filename into a song identity
    Given "downloads/mp3/untagged.mp3" is a copy of the fixture "silence.mp3"
    And "downloads/flac/untagged.flac" is a copy of the fixture "tone.flac"

    When I import the folder "downloads"
    Then 2 tracks are added
    And 0 duplicates are skipped
    And the import finishes without errors
    And the library contains exactly these tracks:
      | title    | audio       |
      | untagged | silence.mp3 |
      | untagged | tone.flac   |

  Scenario: Case differences group candidates without discarding audio
    Given "downloads/first.mp3" is a copy of the fixture "same-song.mp3"
    And "downloads/second.mp3" is a copy of the fixture "case-song.mp3"

    When I import the folder "downloads"
    Then 2 tracks are added
    And 0 duplicates are skipped
    And 1 collision is retained
    And the import finishes without errors
    And the library has 1 browsing group
    And the library contains exactly these tracks:
      | title       | audio         |
      | Fixture NCM | same-song.mp3 |
      | fixture ncm | case-song.mp3 |
