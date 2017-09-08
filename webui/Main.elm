module Main exposing (..)

import Html exposing (Html, button, div, text, ul, li)
import Html.Events exposing (onClick)
import Html.Attributes exposing (..)
import Json.Decode as Decode
import Http


main =
    Html.program { init = init, view = view, update = update, subscriptions = subscriptions }


type alias Model =
    { graph : Graph
    }


init : ( Model, Cmd Msg )
init =
    ( { graph = emptyGraph
      }
    , updateTypes
    )


type Msg
    = RefreshNodes
    | UpdateNodes (Result Http.Error (List Node))
    | RefreshTypes
    | UpdateTypes (Result Http.Error (List NodeType))
    | AddNode NodeType
    | AddedNode (Result Http.Error AddNodeResult)


update : Msg -> Model -> ( Model, Cmd Msg )
update msg model =
    case msg of
        RefreshNodes ->
            ( model, updateNodes )

        UpdateNodes (Ok newNodes) ->
            ( { model | graph = { types = model.graph.types, nodes = newNodes } }, Cmd.none )

        UpdateNodes (Err err) ->
            ( model, Cmd.none )

        RefreshTypes ->
            ( model, updateTypes )

        UpdateTypes (Ok newTypes) ->
            ( { model | graph = { types = newTypes, nodes = model.graph.nodes } }, Cmd.none )

        UpdateTypes (Err err) ->
            ( model, Cmd.none )

        AddNode nodeType ->
            ( model, addNode nodeType )

        AddedNode (Ok nodeId) ->
            ( model, updateNodes )

        AddedNode (Err err) ->
            ( model, Cmd.none )


subscriptions : Model -> Sub Msg
subscriptions model =
    Sub.none


view : Model -> Html Msg
view model =
    div []
        [ button [ onClick RefreshNodes ] [ text "refresh" ]
        , typesView model
        , nodesView model
        ]


typesView : Model -> Html Msg
typesView model =
    div []
        [ text "Node types:"
        , ul [] (List.map typeView model.graph.types)
        ]


typeView : NodeType -> Html Msg
typeView node =
    li [] [ button [ onClick (AddNode node) ] [ text node.name ] ]


nodesView : Model -> Html Msg
nodesView model =
    div [ style [ ( "color", "red" ) ] ] [ text (toString model.graph) ]


type alias Ports =
    { input : List Port
    , output : List Port
    }


type alias Port =
    { edge : Maybe Edge
    }


type alias Edge =
    { nodeId: Int
    , portId: Int
    }


type alias Node =
    { id : Int
    , name : String
    , title : String
    , ports : Ports
    }


type alias NodeType =
    { id : Int
    , name : String
    }


type alias Graph =
    { nodes : List Node
    , types : List NodeType
    }


emptyGraph =
    { nodes = []
    , types = []
    }


updateNodes : Cmd Msg
updateNodes =
    let
        url =
            "http://localhost:8008/node"

        request =
            Http.get url decodeNodes
    in
        Http.send UpdateNodes request


decodeNodes : Decode.Decoder (List Node)
decodeNodes =
    Decode.list
        (Decode.map4 Node
            (Decode.field "id" Decode.int)
            (Decode.field "name" Decode.string)
            (Decode.field "title" Decode.string)
            (Decode.field "ports" decodePorts)
        )

decodePorts : Decode.Decoder Ports
decodePorts =
    Decode.map2 Ports
        (Decode.field "in" (Decode.list decodePort))
        (Decode.field "out" (Decode.list decodePort))


decodePort : Decode.Decoder Port
decodePort =
    Decode.map Port (Decode.maybe (Decode.map2 Edge
        (Decode.at ["edge", "node"] Decode.int)
        (Decode.at ["edge", "port"] Decode.int))
    )


updateTypes : Cmd Msg
updateTypes =
    let
        url =
            "http://localhost:8008/type"

        request =
            Http.get url decodeTypes
    in
        Http.send UpdateTypes request


decodeTypes : Decode.Decoder (List NodeType)
decodeTypes =
    Decode.list
        (Decode.map2 NodeType
            (Decode.field "id" Decode.int)
            (Decode.field "name" Decode.string)
        )


type alias AddNodeResult =
    Result String Int


addNode : NodeType -> Cmd Msg
addNode nodeType =
    let
        url =
            "http://localhost:8008/type/" ++ toString nodeType.id ++ "/new"

        request =
            Http.get url decodeAddNode
    in
        Http.send AddedNode request


decodeAddNode : Decode.Decoder (Result String Int)
decodeAddNode =
    Decode.andThen
        (\status ->
            if status == "ok" then
                Decode.map Ok (Decode.field "id" Decode.int)
            else
                Decode.succeed (Err status)
        )
        (Decode.field "status" Decode.string)
